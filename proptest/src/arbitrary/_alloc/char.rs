//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::char`.

use crate::std_facade::Vec;
#[cfg(feature = "unstable")]
use core::char::{CharTryFromError, ToLowercase, ToUppercase};
use core::char::{EscapeDebug, EscapeDefault, EscapeUnicode, ParseCharError};
use core::iter::once;

use crate::collection::vec;

multiplex_alloc! {
    core::char::DecodeUtf16, std::char::DecodeUtf16,
    core::char::DecodeUtf16Error, std::char::DecodeUtf16Error,
    core::char::decode_utf16, std::char::decode_utf16
}

/// Upper bound on the length of the `Vec<u16>` fed to `decode_utf16`, capping
/// generated `DecodeUtf16` inputs at `u16::MAX` code units.
const VEC_MAX: usize = 65_535;

use crate::arbitrary::{SMapped, any};
#[cfg(feature = "unstable")]
use crate::strategy::Just;
use crate::strategy::statics::static_map;
use crate::strategy::{BoxedStrategy, Strategy as _};

/// Implements `Arbitrary` for a `char`-iterator type produced by a `char`
/// method.
///
/// Draws an arbitrary `char` via `any::<char>()` and maps it through `$mapper`
/// (e.g. `char::escape_debug`) with `static_map`, giving
/// `Strategy = SMapped<char, Self>`.
macro_rules! impl_wrap_char {
    ($type: ty, $mapper: expr) => {
        arbitrary!($type, SMapped<char, Self>;
            static_map(any::<char>(), $mapper));
    };
}

impl_wrap_char!(EscapeDebug, char::escape_debug);
impl_wrap_char!(EscapeDefault, char::escape_default);
impl_wrap_char!(EscapeUnicode, char::escape_unicode);
#[cfg(feature = "unstable")]
impl_wrap_char!(ToLowercase, char::to_lowercase);
#[cfg(feature = "unstable")]
impl_wrap_char!(ToUppercase, char::to_uppercase);

arbitrary!(DecodeUtf16<<Vec<u16> as IntoIterator>::IntoIter>,
    SMapped<Vec<u16>, Self>;
    static_map(vec(any::<u16>(), ..VEC_MAX), decode_utf16)
);

arbitrary!(ParseCharError, BoxedStrategy<Self>;
    static_map(any::<bool>(), |is_two| if is_two { "__" } else { "" })
        .prop_filter_map(
            "invalid char source parses to ParseCharError",
            |source| source.parse::<char>().err(),
        )
        .boxed()
);

#[cfg(feature = "unstable")]
arbitrary!(CharTryFromError, BoxedStrategy<Self>; {
    use core::convert::TryFrom as _;
    Just(0xD800_u32)
        .prop_filter_map(
            "surrogate scalar value cannot convert to char",
            |code| char::try_from(code).err(),
        )
        .boxed()
});

arbitrary!(DecodeUtf16Error, BoxedStrategy<Self>;
    (0xD800_u16..0xE000_u16).prop_filter_map(
        "surrogate code unit decodes to an error",
        |code_unit| decode_utf16(once(code_unit)).next()?.err(),
    )
    .boxed()
);

#[cfg(test)]
mod test {
    use super::*;

    no_panic_test!(
        escape_debug => EscapeDebug,
        escape_default => EscapeDefault,
        escape_unicode => EscapeUnicode,
        parse_char_error => ParseCharError,
        decode_utf16_error => DecodeUtf16Error
    );

    no_panic_test!(
        decode_utf16 => DecodeUtf16<<Vec<u16> as IntoIterator>::IntoIter>
    );

    #[cfg(feature = "unstable")]
    no_panic_test!(
        to_lowercase => ToLowercase,
        to_uppercase => ToUppercase,
        char_try_from_error => CharTryFromError
    );
}
