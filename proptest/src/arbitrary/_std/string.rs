//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::string`.

use core::ops::Range;
use std::iter;
use std::rc::Rc;
use std::slice;
use std::sync::Arc;

use crate::std_facade::Box;
use crate::std_facade::String;
use crate::std_facade::Vec;

multiplex_alloc! {
    alloc::string::FromUtf8Error, ::std::string::FromUtf8Error,
    alloc::string::FromUtf16Error, ::std::string::FromUtf16Error
}

use crate::arbitrary::Arbitrary;
use crate::arbitrary::StrategyFor;
use crate::arbitrary::any;
use crate::arbitrary::any_with;
use crate::collection;
use crate::strategy::BoxedStrategy;
use crate::strategy::Just;
use crate::strategy::LazyJust;
use crate::strategy::MapInto;
use crate::strategy::Strategy;
use crate::strategy::statics::static_map;
use crate::string::StringParam;

impl Arbitrary for String {
  type Parameters = StringParam;
  type Strategy = &'static str;

  /// ## Panics
  ///
  /// This implementation panics if the input is not a valid regex proptest
  /// can handle.
  fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
    args.into()
  }
}

/// Implements `Arbitrary` for a DST-pointer wrapper around `str`
/// (`Box<str>`, `Rc<str>`, `Arc<str>`).
///
/// Each wrapper reuses `String`'s strategy and `StringParam`, mapping the
/// generated `String` into the wrapper with `prop_map_into`.
macro_rules! dst_wrapped {
    ($($w: ident),*) => {
        $(std_arbitrary_with_params!($w<str>,
            MapInto<StrategyFor<String>, Self>, StringParam;
            args => any_with::<String>(args).prop_map_into()
        );)*
    };
}

dst_wrapped!(Box, Rc, Arc);

arbitrary!(FromUtf16Error, BoxedStrategy<Self>;
    LazyJust::new(|| {
        loop {
            if let Err(error) = String::from_utf16(&[0xD800_u16]) {
                break error;
            }
        }
    }).boxed()
);

// This is a void-like type, it needs to be handled by the user of
// the type by simply never constructing the variant in an enum or for
// structs by inductively not generating the struct.
// The same applies to ! and Infallible.
// generator!(ParseError, || panic!());

arbitrary!(FromUtf8Error, BoxedStrategy<Self>;
    not_utf8_bytes(true)
        .boxed()
        .prop_filter_map(
            "invalid UTF-8 bytes construct FromUtf8Error",
            |bytes| String::from_utf8(bytes).err(),
        )
        .boxed()
);

/// This strategy produces sequences of bytes that are guaranteed to be illegal
/// wrt. UTF-8 with the goal of producing a suffix of bytes in the end of
/// an otherwise legal UTF-8 string that causes the string to be illegal.
/// This is used primarily to generate the `Utf8Error` type and similar.
pub(super) fn not_utf8_bytes(allow_null: bool) -> impl Strategy<Value = Vec<u8>> {
  let prefix = collection::vec(any::<char>(), ..usize::from(u16::MAX));
  let suffix = gen_el_bytes(allow_null);
  (prefix, suffix).prop_map(move |(prefix_bytes, el_bytes)| {
    let iter = prefix_bytes.iter();
    let string: String = if allow_null {
      iter.collect()
    } else {
      iter.filter(|&&x| x != '\u{0}').collect()
    };
    let mut bytes = string.into_bytes();
    bytes.extend(&el_bytes);
    bytes
  })
}

/// Stands for `error_length` bytes and contains a suffix of bytes that
/// will cause the whole string to become invalid UTF-8.
/// See `gen_el_bytes` for more details.
#[derive(Debug)]
enum ELBytes {
  /// A one-byte invalid-UTF-8 tail suffix.
  B1([u8; 1]),
  /// A two-byte invalid-UTF-8 tail suffix.
  B2([u8; 2]),
  /// A three-byte invalid-UTF-8 tail suffix.
  B3([u8; 3]),
  /// A four-byte invalid-UTF-8 tail suffix.
  B4([u8; 4]),
}

impl<'a> IntoIterator for &'a ELBytes {
  type Item = u8;
  type IntoIter = iter::Copied<slice::Iter<'a, u8>>;
  fn into_iter(self) -> Self::IntoIter {
    (match *self {
      ELBytes::B1(ref bytes) => bytes.iter(),
      ELBytes::B2(ref bytes) => bytes.iter(),
      ELBytes::B3(ref bytes) => bytes.iter(),
      ELBytes::B4(ref bytes) => bytes.iter(),
    })
    .copied()
  }
}

/// Wraps a single byte as the one-byte `ELBytes` suffix arm.
const fn b1(byte: u8) -> ELBytes {
  ELBytes::B1([byte])
}

/// Wraps a leading-byte pair as the two-byte `ELBytes` suffix arm.
fn b2(bytes: (u8, u8)) -> ELBytes {
  ELBytes::B2(bytes.into())
}

/// Wraps a pair-plus-continuation triple as the three-byte `ELBytes`
/// suffix arm.
const fn b3(bytes: ((u8, u8), u8)) -> ELBytes {
  ELBytes::B3([(bytes.0).0, (bytes.0).1, bytes.1])
}

/// Wraps the widest tuple as the four-byte `ELBytes` suffix arm.
#[allow(
  clippy::single_call_fn,
  reason = "maps only the error_len = Some(3), width = 4 strategy arm of gen_el_bytes"
)]
const fn b4(bytes: ((u8, u8), u8, u8)) -> ELBytes {
  ELBytes::B4([(bytes.0).0, (bytes.0).1, bytes.1, bytes.2])
}

/// Lead-byte category for valid 3-byte UTF-8 sequences.
#[derive(Clone, Copy, Debug)]
enum Width3Lead {
  /// The minimum 3-byte lead byte, whose second byte starts at `0xA0`.
  Min,
  /// The middle 3-byte lead bytes, whose second byte spans all
  /// continuation values.
  Middle(u8),
  /// The maximum 3-byte lead bytes, whose second byte stops before `0xA0`.
  Max(u8),
}

impl Width3Lead {
  /// Return the concrete first byte represented by this category.
  const fn byte(self) -> u8 {
    match self {
      Self::Min => 0xE0,
      Self::Middle(byte) | Self::Max(byte) => byte,
    }
  }

  /// Return the valid second-byte range for this first-byte category.
  const fn valid_second(self) -> Range<u8> {
    match self {
      Self::Min => 0xA0_u8..0xC0_u8,
      Self::Middle(_) => 0x80_u8..0xC0_u8,
      Self::Max(_) => 0x80_u8..0xA0_u8,
    }
  }

  /// Return a strategy for second bytes that make this prefix invalid.
  fn invalid_second(self, start_byte: u8) -> impl Strategy<Value = u8> {
    match self {
      Self::Min => prop_oneof![start_byte..0xA0_u8, 0xC0_u8..],
      Self::Middle(_) => {
        prop_oneof![start_byte..0x80_u8, 0xC0_u8..]
      }
      Self::Max(_) => prop_oneof![start_byte..0x80_u8, 0xA0_u8..],
    }
  }
}

/// Lead-byte category for valid 4-byte UTF-8 sequences.
#[derive(Clone, Copy, Debug)]
enum Width4Lead {
  /// The minimum 4-byte lead byte, whose second byte starts at `0x90`.
  Min,
  /// The middle 4-byte lead bytes.
  Middle(u8),
  /// The maximum 4-byte lead byte, whose second byte stops before `0x90`.
  Max,
}

impl Width4Lead {
  /// Return the concrete first byte represented by this category.
  const fn byte(self) -> u8 {
    match self {
      Self::Min => 0xF0,
      Self::Middle(byte) => byte,
      Self::Max => 0xF4,
    }
  }

  /// Return the valid second-byte range for this first-byte category.
  const fn valid_second(self) -> Range<u8> {
    match self {
      Self::Min => 0x90_u8..0xA0_u8,
      Self::Middle(_) => 0x80_u8..0xA0_u8,
      Self::Max => 0x80_u8..0x90_u8,
    }
  }

  /// Return a strategy for second bytes that make this prefix invalid.
  fn invalid_second(self, start_byte: u8) -> impl Strategy<Value = u8> {
    match self {
      Self::Min => prop_oneof![start_byte..0x90_u8, 0xA0_u8..],
      Self::Middle(_) => {
        prop_oneof![start_byte..0x80_u8, 0xA0_u8..]
      }
      Self::Max => prop_oneof![start_byte..0x80_u8, 0x90_u8..],
    }
  }
}

// By analysis of run_utf8_validation defined at:
// https://doc.rust-lang.org/nightly/src/core/str/mod.rs.html#1429
// we know that .error_len() \in {None, Some(1), Some(2), Some(3)}.
// We represent this with the range [0..4) and generate a valid
// sequence from that.
/// Builds a strategy over `ELBytes` suffixes guaranteed to turn an
/// otherwise-valid UTF-8 string into invalid UTF-8.
///
/// The arms mirror `core`'s `run_utf8_validation`, covering every `error_len`
/// outcome (`None`, `Some(1)`, `Some(2)`, `Some(3)`) across the 2-, 3-, and
/// 4-byte sequence widths. When `allow_null` is `false`, the nul byte is kept
/// out of the generated bytes.
#[allow(
  clippy::single_call_fn,
  reason = "the ELBytes strategy covering every UTF-8 validation error-length arm"
)]
fn gen_el_bytes(allow_null: bool) -> impl Strategy<Value = ELBytes> {
  // https://tools.ietf.org/html/rfc3629
  // static UTF8_CHAR_WIDTH: [u8; 256] = [
  // 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
  // 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, // 0x1F
  // 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
  // 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, // 0x3F
  // 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
  // 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, // 0x5F
  // 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,
  // 1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1, // 0x7F
  // 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
  // 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, // 0x9F
  // 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
  // 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0, // 0xBF
  // 0,0,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
  // 2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2, // 0xDF
  // 3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3, // 0xEF
  // 4,4,4,4,4,0,0,0,0,0,0,0,0,0,0,0, // 0xFF
  // ];
  //
  // Mask of the value bits of a continuation byte.
  // const CONT_MASK: u8 = 0b0011_1111;
  // Value of the tag bits (tag mask is !CONT_MASK) of a continuation byte.
  // const TAG_CONT_U8: u8 = 0b1000_0000;

  // Continuation byte:
  let succ_byte = 0x80_u8..0xC0_u8;

  // Do we allow the nul byte or not?
  let start_byte = u8::from(!allow_null);

  // Invalid continuation byte:
  let fail_byte = prop_oneof![start_byte..0x7F_u8, 0xC1_u8..];

  // Matches zero in the UTF8_CHAR_WIDTH table above.
  let byte0_w0 = prop_oneof![0x80_u8..0xC0_u8, 0xF5_u8..];

  // Start of a 3 (width) byte sequence:
  // Leads here: https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1479
  let byte0_w2 = 0xC2_u8..0xE0_u8;

  // Start of a 3 (width) byte sequence:
  // https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1484
  // See the left column in the match.
  let width3_lead = prop_oneof![
    Just(Width3Lead::Min),
    static_map(0xE1_u8..0xED_u8, Width3Lead::Middle),
    static_map(0xED_u8..0xF0_u8, Width3Lead::Max),
  ];

  // Start of a 4 (width) byte sequence:
  // https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1495
  // See the left column in the match.
  let width4_lead = prop_oneof![
    Just(Width4Lead::Min),
    static_map(0xF1_u8..0xF4_u8, Width4Lead::Middle),
    Just(Width4Lead::Max),
  ];

  // The 2 first (valid) bytes of a 3 (width) byte sequence:
  // The first byte is `width3_lead`. The second is the ones produced on the right.
  let width3_prefix = width3_lead
    .clone()
    .prop_flat_map(|lead| (Just(lead.byte()), lead.valid_second()));

  // In a 3 (width) byte sequence, an invalid second byte is chosen such that
  // it will yield an error length of Some(1). The second byte is on
  // the right of the match arms.
  let width3_invalid_second = width3_lead
    .clone()
    .prop_flat_map(move |lead| (Just(lead.byte()), lead.invalid_second(start_byte)));

  // In a 4 (width) byte sequence, an invalid second byte is chosen such that
  // it will yield an error length of Some(1). The second byte is on
  // the right of the match arms.
  let width4_invalid_second = width4_lead
    .clone()
    .prop_flat_map(move |lead| (Just(lead.byte()), lead.invalid_second(start_byte)));

  // The 2 first (valid) bytes of a 4 (width) byte sequence:
  // The first byte is `width4_lead`. The second is the ones produced on the right.
  let width4_prefix = width4_lead
    .clone()
    .prop_flat_map(|lead| (Just(lead.byte()), lead.valid_second()));

  prop_oneof![
    // error_len = None
    // These are all happen when next!() fails to provide a byte.
    prop_oneof![
      // width = 2
      // lacking 1 bytes:
      static_map(byte0_w2.clone(), b1),
      // width = 3
      // lacking 2 bytes:
      width3_lead.prop_map(|lead| b1(lead.byte())),
      // lacking 1 bytes:
      static_map(width3_prefix.clone(), b2),
      // width = 4
      // lacking 3 bytes:
      width4_lead.prop_map(|lead| b1(lead.byte())),
      // lacking 2 bytes:
      static_map(width4_prefix.clone(), b2),
      // lacking 1 byte:
      static_map((width4_prefix.clone(), succ_byte.clone()), b3),
    ],
    // error_len = Some(1)
    prop_oneof![
      // width = 1 is not represented.
      // width = 0
      // path taken:
      // https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1508
      static_map(byte0_w0, b1),
      // width = 2
      // path taken:
      // https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1480
      static_map((byte0_w2, fail_byte.clone()), b2),
      // width = 3
      // path taken:
      // https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1488
      static_map(width3_invalid_second, b2),
      // width = 4
      // path taken:
      // https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1499
      static_map(width4_invalid_second, b2),
    ],
    // error_len = Some(2)
    static_map(
      prop_oneof![
        // width = 3
        // path taken:
        // https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1491
        (width3_prefix, fail_byte.clone()),
        // width = 4
        // path taken:
        // https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1502
        (width4_prefix.clone(), fail_byte.clone())
      ],
      b3
    ),
    // error_len = Some(3), width = 4
    // path taken:
    // https://doc.rust-lang.org/1.23.0/src/core/str/mod.rs.html#1505
    static_map((width4_prefix, succ_byte, fail_byte), b4),
  ]
  .boxed()
}

#[cfg(test)]
mod test {
  use super::*;

  no_panic_test!(
      string  => String,
      str_box => Box<str>,
      str_rc  => Rc<str>,
      str_arc => Arc<str>,
      from_utf16_error => FromUtf16Error,
      from_utf8_error => FromUtf8Error
  );
}
