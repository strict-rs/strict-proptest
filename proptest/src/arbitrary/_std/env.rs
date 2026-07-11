//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::env`.

use std::env::{
    Args, ArgsOs, JoinPathsError, VarError, Vars, VarsOs, args, args_os,
    join_paths, vars, vars_os,
};
use std::ffi::OsString;
use std::iter::once;

use crate::arbitrary::SFnPtrMap;
use crate::strategy::statics::static_map;
use crate::strategy::{
    BoxedStrategy, Just, Strategy, TupleUnion, WeightedStrategy,
};

// FIXME: SplitPaths when lifetimes in strategies are possible.

lazy_just!(
    Args, args;
    ArgsOs, args_os;
    Vars, vars;
    VarsOs, vars_os
);

/// Produces a `JoinPathsError` by asking `join_paths` to join a single entry
/// containing the platform's forbidden path-separator character.
#[cfg(not(target_os = "windows"))]
#[allow(
    clippy::single_call_fn,
    reason = "forge a JoinPathsError by joining a single entry with the platform's illegal separator"
)]
fn jpe() -> JoinPathsError {
    loop {
        if let Err(error) = join_paths(once(":")) {
            return error;
        }
    }
}

/// Produces a `JoinPathsError` by asking `join_paths` to join a single entry
/// containing the platform's forbidden path-separator character.
#[cfg(target_os = "windows")]
fn jpe() -> JoinPathsError {
    loop {
        if let Err(error) = join_paths(once("\"")) {
            return error;
        }
    }
}

lazy_just!(JoinPathsError, jpe);

// Algorithm from: https://stackoverflow.com/questions/47749164
#[cfg(any(target_os = "windows", test))]
#[allow(
    clippy::single_call_fn,
    reason = "corrupt one code unit of a UTF-16 buffer to build an invalid-Unicode OsString source"
)]
fn make_utf16_invalid(buf: &mut [u16], pos: usize) {
    let Some(current) = buf.get(pos).copied() else {
        return;
    };

    // If first elem or previous entry is not a leading surrogate.
    let gen_trail = pos
        .checked_sub(1)
        .and_then(|previous| buf.get(previous))
        .is_none_or(|previous| 0xd800 != (previous & 0xfc00));
    // If last element or succeeding entry is not a traililng surrogate.
    let gen_lead = pos
        .checked_add(1)
        .and_then(|next| buf.get(next))
        .is_none_or(|next| 0xdc00 != (next & 0xfc00));
    let (force_bits_mask, force_bits_value) = if gen_trail {
        if gen_lead {
            // Trailing or leading surrogate.
            (0xf800, 0xd800)
        } else {
            // Trailing surrogate.
            (0xfc00, 0xdc00)
        }
    } else {
        // Leading surrogate.
        // Note that `gen_lead` and `gen_trail` could both be false here if `pos`
        // lies exactly between a leading and a trailing surrogate. In this
        // case, it doesn't matter what we do because the UTF-16 will be
        // invalid regardless, so just always force a leading surrogate.
        (0xfc00, 0xd800)
    };
    if let Some(slot) = buf.get_mut(pos) {
        *slot = (current & !force_bits_mask) | force_bits_value;
    }
}

/// `Arbitrary` impl for `std::env::VarError`.
///
/// Kept in its own module (excluded on `wasm32`) so the platform-specific
/// machinery for fabricating a non-Unicode `OsString` stays contained.
#[cfg(not(target_arch = "wasm32"))]
mod var_error {
    use super::{
        BoxedStrategy, Just, OsString, SFnPtrMap, Strategy, TupleUnion,
        VarError, WeightedStrategy, static_map,
    };

    /// Generates the set of `WTF-16 \ UTF-16` and makes
    /// an `OsString` that is not a valid `String` from it.
    #[cfg(target_os = "windows")]
    fn osstring_invalid_string() -> impl Strategy<Value = OsString> {
        use std::os::windows::ffi::OsStringExt;
        let size = 1..u16::MAX as usize;
        let vec_gen = crate::collection::vec(..u16::MAX, size.clone());
        (size, vec_gen).prop_map(|(p, mut sbuf)| {
            // Not quite a uniform distribution due to clamping,
            // but probably good enough
            let p = ::std::cmp::min(p, sbuf.len() - 1);
            make_utf16_invalid(&mut sbuf, p);
            loop {
                if let Err(error) =
                    OsString::from_wide(sbuf.as_slice()).into_string()
                {
                    break error;
                }
            }
        })
    }

    /// Generates an `OsString` that is not a valid `String`.
    ///
    /// Wraps non-UTF-8 bytes (from `not_utf8_bytes`) with
    /// `OsStringExt::from_vec`, so converting the result into a `String`
    /// always fails; this feeds `VarError::NotUnicode`.
    #[cfg(not(target_os = "windows"))]
    #[allow(
        clippy::single_call_fn,
        reason = "wrap non-UTF-8 bytes into an OsString so into_string always errors on non-Windows targets"
    )]
    fn osstring_invalid_string() -> impl Strategy<Value = OsString> {
        use crate::arbitrary::_std::string::not_utf8_bytes;
        use std::os::unix::ffi::OsStringExt as _;
        static_map(not_utf8_bytes(true), OsString::from_vec)
    }

    arbitrary!(VarError,
        TupleUnion<(
            WeightedStrategy<Just<Self>>,
            WeightedStrategy<SFnPtrMap<BoxedStrategy<OsString>, Self>>
        )>;
        prop_oneof![
            Just(VarError::NotPresent),
            static_map(osstring_invalid_string().boxed(), VarError::NotUnicode)
        ]
    );
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::num;
    use crate::strict::{
        TestResult, ensure_property_with_config, strict_default_config,
    };
    use crate::test_runner::Config;

    no_panic_test!(
        args => Args,
        args_os => ArgsOs,
        vars => Vars,
        vars_os => VarsOs,
        join_paths_error => JoinPathsError,
        var_error => VarError
    );

    #[test]
    fn make_utf16_invalid_doesnt_panic() -> TestResult {
        // Keep the legacy 65536-case sweep over (buffer, position); the
        // strict defaults supply deterministic seeding and disable failure
        // persistence.
        let config = Config {
            cases: 65536,
            ..strict_default_config()
        };
        ensure_property_with_config(
            &([num::u16::ANY; 3], 0_usize..3),
            "make_utf16_invalid handles every position in a 3-element buffer",
            config,
            |(mut buf, pos)| {
                make_utf16_invalid(&mut buf, pos);
                Ok(())
            },
        )
    }
}
