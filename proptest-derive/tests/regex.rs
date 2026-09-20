// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for the `#[proptest(regex = ...)]` field
//! attribute.
//!
//! Derives `Arbitrary` for structs and enum variants whose `String`,
//! `Vec<u8>`, and custom `StrategyFromRegex` fields carry a `regex`
//! modifier in every spelling (`= "..."`, `(...)`, a `fn` path, and raw
//! strings), some combined with `filter`. Each generated field is checked
//! to match the regex it was given.

#[cfg(test)]
mod tests {
  mod support;

  use std::num::ParseIntError;

  use proptest::prelude::Arbitrary;
  use proptest::prelude::BoxedStrategy;
  use proptest::prelude::Strategy as _;
  use proptest::prelude::any;
  use proptest::strict::ensure_property;
  use proptest::string::StrategyFromRegex;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;
  use support::assert_arbitrary;

  /// Original field owner and the native decimal parse result.
  type Observation<T> = (T, Result<u8, ParseIntError>);
  /// A failed field contract keeps its complete owner and parsing result.
  type AdherenceFailure<T> = Box<PredicateFailure<Observation<T>>>;
  /// Property execution retains every field and parsing outcome.
  type Checked<T> = PropertyResult<T, Observation<T>, AdherenceFailure<T>>;
  /// Borrow the six regex-controlled fields without consuming their owner.
  type Fields<'a> = (&'a str, &'a str, &'a str, &'a [u8], &'a [u8], &'a [u8]);

  const fn mk_regex() -> &'static str {
    "[0-9][0-9]"
  }

  /// Apply the same named-field attributes to a struct and an enum variant.
  macro_rules! named_regex_fixture {
    ($kind:ident $name:ident $(::$variant:ident)?) => {
      named_regex_fixture!(@$kind $name $(::$variant)? {
        #[proptest(regex = "a+")]
        foo: String,
        #[proptest(regex("b+"))]
        bar: String,
        #[proptest(regex(mk_regex))]
        baz: String,
        #[proptest(regex = "(a|b)+")]
        quux: Vec<u8>,
        #[proptest(regex("[abc]+"), filter("|bytes| bytes.len() < 4"))]
        wibble: Vec<u8>,
        #[proptest(regex(mk_regex))]
        wobble: Vec<u8>,
      });
    };
    (@struct $name:ident $fields:tt) => {
      #[derive(Debug, Arbitrary)]
      struct $name $fields
    };
    (@enum $name:ident::$variant:ident $fields:tt) => {
      #[derive(Debug, Arbitrary)]
      enum $name { $variant $fields }
    };
  }

  named_regex_fixture!(struct T0);

  #[derive(Debug, Arbitrary)]
  struct T1(
    #[proptest(regex = "a+")] String,
    #[proptest(regex("b+"))] String,
    #[proptest(regex(mk_regex))] String,
    #[proptest(regex = "(a|b)+")] Vec<u8>,
    #[proptest(regex("[abc]+"), filter("|bytes| bytes.len() < 4"))] Vec<u8>,
    #[proptest(regex(mk_regex))] Vec<u8>,
  );

  #[derive(Debug, Arbitrary)]
  struct T1r(
    #[proptest(regex = r"\x61+")] String,
    #[proptest(regex(r"\x62+"))] String,
    #[proptest(regex(mk_regex))] String,
    #[proptest(regex = r"(\x61|\x62)+")] Vec<u8>,
    #[proptest(regex(r"[\x61\x62\x63]+"), filter("|bytes| bytes.len() < 4"))] Vec<u8>,
    #[proptest(regex(mk_regex))] Vec<u8>,
  );

  // enum:

  named_regex_fixture!(enum T2::V0);

  #[derive(Debug, Arbitrary)]
  enum T3 {
    V0(
      #[proptest(regex = "a+")] String,
      #[proptest(regex("b+"))] String,
      #[proptest(regex(mk_regex))] String,
      #[proptest(regex = "(a|b)+")] Vec<u8>,
      #[proptest(regex("[abc]+"), filter("|bytes| bytes.len() < 4"))] Vec<u8>,
      #[proptest(regex(mk_regex))] Vec<u8>,
    ),
  }

  // Show that it works for new types and that `String` | `Vec<u8>` isn't
  // hardcoded into the logic:

  #[derive(Debug)]
  struct NewString(String);

  impl StrategyFromRegex for NewString {
    type Strategy = BoxedStrategy<Self>;

    fn from_regex(regex: &str) -> Self::Strategy {
      String::from_regex(regex).prop_map(NewString).boxed()
    }
  }

  #[derive(Debug, Arbitrary)]
  struct T4(#[proptest(regex = "a+")] NewString);

  /// The positive closure of the ASCII letter a contains no empty string.
  fn is_aplus(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|letter| letter == 'a')
  }

  /// Check the complete owner while preserving the fallible decimal observation.
  fn check_adherence<T: Arbitrary>(context: &'static str, fields: fn(&T) -> Fields<'_>) -> Checked<T> {
    ensure_property(&any::<T>(), context, |sample| {
      let parsed = fields(&sample).2.parse::<u8>();
      ensure_that((sample, parsed), "every regex and length filter holds", |observed| {
        let (x0, x1, _, y0, y1, y2) = fields(&observed.0);
        is_aplus(x0)
          && !x1.is_empty()
          && x1.chars().all(|letter| letter == 'b')
          && observed.1.as_ref().is_ok_and(|value| *value < 100)
          && !y0.is_empty()
          && y0.iter().all(|byte| b"ab".contains(byte))
          && !y1.is_empty()
          && y1.len() < 4
          && y1.iter().all(|byte| b"abc".contains(byte))
          && !y2.is_empty()
          && y2.iter().all(u8::is_ascii_digit)
      })
      .map_err(Box::new)
    })
  }

  #[test]
  fn t0_adhering_to_regex() -> Checked<T0> {
    check_adherence("named struct regex fields adhere to their regexes", |sample: &T0| {
      (&sample.foo, &sample.bar, &sample.baz, &sample.quux, &sample.wibble, &sample.wobble)
    })
  }

  #[test]
  fn t1_adhering_to_regex() -> Checked<T1> {
    check_adherence("tuple struct regex fields adhere to their regexes", |sample: &T1| {
      (&sample.0, &sample.1, &sample.2, &sample.3, &sample.4, &sample.5)
    })
  }

  #[test]
  fn t1_r_adhering_to_regex() -> Checked<T1r> {
    check_adherence("raw-string regex fields adhere to their regexes", |sample: &T1r| {
      (&sample.0, &sample.1, &sample.2, &sample.3, &sample.4, &sample.5)
    })
  }

  #[test]
  fn t2_adhering_to_regex() -> Checked<T2> {
    check_adherence("struct-variant regex fields adhere to their regexes", |sample| {
      let T2::V0 {
        ref foo,
        ref bar,
        ref baz,
        ref quux,
        ref wibble,
        ref wobble,
      } = *sample;
      (foo, bar, baz, quux, wibble, wobble)
    })
  }

  #[test]
  fn t3_adhering_to_regex() -> Checked<T3> {
    check_adherence("tuple-variant regex fields adhere to their regexes", |sample| {
      let T3::V0(ref x0, ref x1, ref x2, ref y0, ref y1, ref y2) = *sample;
      (x0, x1, x2, y0, y1, y2)
    })
  }

  #[test]
  fn t4_adhering_to_regex() -> PropertyResult<T4, T4, PredicateFailure<T4>> {
    ensure_property(&any::<T4>(), "a custom StrategyFromRegex type adheres to its regex", |generated| {
      ensure_that(generated, "the custom string is non-empty and contains only a", |sample| {
        is_aplus(&(sample.0).0)
      })
    })
  }

  assert_arbitrary!(T0, T1, T2, T3, T4,);
}
