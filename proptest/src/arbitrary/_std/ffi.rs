//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::ffi`.

use crate::std_facade::{Box, String, Vec};
use std::ffi::*;
use std::ops::RangeInclusive;

use crate::arbitrary::*;
use crate::collection::*;
use crate::strategy::statics::static_map;
use crate::strategy::*;

use super::string::not_utf8_bytes;

std_arbitrary_with_params!(CString,
    SFnPtrMap<VecStrategy<RangeInclusive<u8>>, Self>, SizeRange;
    args => static_map(vec(1..=u8::MAX, args), |vec| {
        // Could use: Self::from_vec_unchecked(vec) safely.
        Self::new(vec).unwrap()
    })
);

std_arbitrary_with_params!(OsString, MapInto<StrategyFor<String>, Self>,
    <String as Arbitrary>::Parameters;
    args => any_with::<String>(args).prop_map_into()
);

/// Implements `Arbitrary` for a DST-pointer wrapper around `CStr`/`OsStr`.
///
/// For each wrapper `W`, `W<CStr>` maps from an arbitrary `CString` and
/// `W<OsStr>` from an arbitrary `OsString`, using `prop_map_into` so the
/// wrapper reuses the owned type's strategy and parameters.
macro_rules! dst_wrapped {
    ($($w: ident),*) => {
        $(std_arbitrary_with_params!($w<CStr>,
            MapInto<StrategyFor<CString>, Self>, SizeRange;
            args => any_with::<CString>(args).prop_map_into()
        );)*
        $(std_arbitrary_with_params!($w<OsStr>,
            MapInto<StrategyFor<OsString>, Self>,
            <String as Arbitrary>::Parameters;
            args => any_with::<OsString>(args).prop_map_into()
        );)*
    };
}

dst_wrapped!(Box);

#[cfg(feature = "unstable")]
use std::rc::Rc;
#[cfg(feature = "unstable")]
use std::sync::Arc;
#[cfg(feature = "unstable")]
dst_wrapped!(Rc, Arc);

arbitrary!(FromBytesWithNulError, SMapped<Option<u16>, Self>; {
    static_map(any::<Option<u16>>(), |opt_pos| {
        // We make some assumptions about the internal structure of
        // FromBytesWithNulError. However, these assumptions do not
        // involve any non-public API.
        if let Some(pos) = opt_pos {
            let pos = pos as usize;
            // Allocate pos + 2 so that we never reallocate:
            let mut bytes = Vec::<u8>::with_capacity(pos + 2);
            bytes.extend(core::iter::repeat_n(1, pos));
            bytes.push(0);
            bytes.push(1);
            CStr::from_bytes_with_nul(bytes.as_slice()).unwrap_err()
        } else {
            CStr::from_bytes_with_nul(b"").unwrap_err()
        }
    })
});

arbitrary!(IntoStringError, SFnPtrMap<BoxedStrategy<Vec<u8>>, Self>;
    static_map(not_utf8_bytes(false).boxed(), |bytes|
        CString::new(bytes).unwrap().into_string().unwrap_err()
    )
);

#[cfg(test)]
mod test {
    use strict_test_support::{TestFailure, ensure, ensure_some};

    use super::*;
    use crate::arbitrary::any_with;
    use crate::collection::size_range;
    use crate::strategy::{Strategy, ValueTree};
    use crate::test_runner::TestRunner;

    no_panic_test!(
        c_string => CString,
        os_string => OsString,
        box_c_str => Box<CStr>,
        box_os_str => Box<OsStr>,
        into_string_error => IntoStringError,
        from_bytes_with_nul => FromBytesWithNulError
    );
    #[cfg(feature = "unstable")]
    no_panic_test!(
        rc_c_str => Rc<CStr>,
        rc_os_str => Rc<OsStr>,
        arc_c_str => Arc<CStr>,
        arc_os_str => Arc<OsStr>
    );

    fn ensure_c_string_contract(
        bounds: SizeRange,
        expected: impl Fn(usize) -> bool,
        context: &'static str,
    ) -> Result<(), TestFailure> {
        let mut runner = TestRunner::deterministic();
        let strategy = any_with::<CString>(bounds);
        for _ in 0..64 {
            let value = ensure_some(
                strategy.new_tree(&mut runner).ok(),
                "CString strategy generates a value tree",
            )?
            .current();
            let bytes = value.as_bytes();
            ensure(expected(bytes.len()), context)?;
            ensure(
                !bytes.contains(&0),
                "generated CString bytes contain no interior NUL",
            )?;
        }
        Ok(())
    }

    #[test]
    fn c_string_respects_zero_length_range() -> Result<(), TestFailure> {
        ensure_c_string_contract(
            size_range(0..=0),
            |len| len == 0,
            "zero-length CString range generates empty byte strings",
        )
    }

    #[test]
    fn c_string_respects_bounded_length_range() -> Result<(), TestFailure> {
        ensure_c_string_contract(
            size_range(3..=5),
            |len| (3..=5).contains(&len),
            "bounded CString range generates lengths inside the range",
        )
    }
}
