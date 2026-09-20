// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Grab-bag coverage for `#[proptest(...)]` combinations not exercised by the
//! per-modifier test files.
//!
//! The derived enums mix container-level `params` with variant-level
//! `value`, `strategy`, `no_params`, and `params` attributes on each variant,
//! and each property confirms the payloads stay reachable and pinned.

#[cfg(test)]
mod tests {
  use proptest::prelude::Arbitrary;
  use proptest::prelude::any;
  use proptest::prelude::any_with;
  use proptest::strategy::Just;
  use proptest::strict::ensure_property;
  use proptest::test_runner::PropertyResult;
  use proptest_derive::Arbitrary;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  // TODO: An idea.
  // #[derive(Debug, Arbitrary)]
  // #[proptest(with = "Foo::ctor(1337, :usize:.other_fn(:f64:, #0..7#))")]
  // struct Foo {
  // ..
  // }

  #[derive(Default)]
  struct Complex;

  #[derive(Debug, Arbitrary)]
  #[proptest(params(Complex))]
  enum Foo {
    #[proptest(value = "Foo::F0(1, 1)")]
    F0(usize, u8),
  }

  impl Foo {
    const fn payload(&self) -> (usize, u8) {
      match self {
        &Self::F0(left, right) => (left, right),
      }
    }
  }

  #[derive(Clone, Debug, Arbitrary)]
  #[proptest(params = "usize")]
  enum Custom {
    Unit,
    #[proptest(strategy = "Just(Custom::Fixed(1))")]
    Fixed(usize),
  }

  impl Custom {
    const fn payload(&self) -> Option<usize> {
      match *self {
        Self::Unit => None,
        Self::Fixed(payload) => Some(payload),
      }
    }
  }

  #[derive(Clone, Debug, Arbitrary)]
  enum Bobby {
    #[proptest(no_params)]
    Defaulted(usize),
    #[proptest(no_params, value = "Bobby::Valued(1)")]
    Valued(usize),
    #[proptest(no_params, strategy = "Just(Bobby::Strategized(1))")]
    Strategized(usize),
    #[proptest(params(Complex), value = "Bobby::ParamValued(1)")]
    ParamValued(usize),
    #[proptest(params(Complex), strategy = "Just(Bobby::ParamStrategized(1))")]
    ParamStrategized(usize),
  }

  impl Bobby {
    const fn payload(&self) -> usize {
      match self {
        &Self::Defaulted(payload)
        | &Self::Valued(payload)
        | &Self::Strategized(payload)
        | &Self::ParamValued(payload)
        | &Self::ParamStrategized(payload) => payload,
      }
    }
  }

  #[derive(Clone, Debug, Arbitrary)]
  enum Quux {
    Bare(#[proptest(no_params)] usize),
    Pair(usize, String),
    #[proptest(value = "Quux::PinnedPair(2, \"a\".into())")]
    PinnedPair(usize, String),
    #[proptest(strategy = "Just(Quux::PinnedWord(1337))")]
    PinnedWord(u32),
    Braced {
      #[proptest(strategy = "10usize..20usize")]
      _foo: usize,
    },
  }

  impl Quux {
    const fn payload_score(&self) -> usize {
      match self {
        &Self::Bare(payload)
        | &Self::Braced {
          _foo: payload,
        } => payload,
        &Self::Pair(payload, ref text) | &Self::PinnedPair(payload, ref text) => payload.saturating_add(text.len()),
        &Self::PinnedWord(1337) => 1337,
        &Self::PinnedWord(_) => 0,
      }
    }
  }

  #[test]
  fn foo_value_constructor_sets_payload() -> PropertyResult<Foo, Foo, PredicateFailure<Foo>> {
    ensure_property(&any::<Foo>(), "a variant value constructor pins the payload", |generated| {
      ensure_that(generated, "both differently typed payloads are pinned to one", |value| {
        value.payload() == (1, 1)
      })
    })
  }

  #[test]
  fn a_custom_strategy_sets_c_payload() -> PropertyResult<Custom, Custom, PredicateFailure<Custom>> {
    ensure_property(&any_with::<Custom>(0_usize), "a variant strategy pins the C payload", |generated| {
      ensure_that(generated, "the strategy-built payload is one whenever present", |value| {
        value.payload().is_none_or(|payload| payload == 1)
      })
    })
  }

  #[test]
  fn bobby_attributes_keep_payloads_reachable() -> PropertyResult<Bobby, Bobby, PredicateFailure<Bobby>> {
    ensure_property(
      &any::<Bobby>(),
      "per-variant params spellings keep payloads reachable",
      |generated| {
        ensure_that(
          generated,
          "default payloads are retained and explicit payloads are one",
          |value| match *value {
            Bobby::Defaulted(payload) => value.payload() == payload,
            Bobby::Valued(_) | Bobby::Strategized(_) | Bobby::ParamValued(_) | Bobby::ParamStrategized(_) => value.payload() == 1,
          },
        )
      },
    )
  }

  /// The generated variant and its observed score remain together in the report.
  type ScoredQuux = PropertyResult<Quux, (Quux, usize), PredicateFailure<(Quux, usize)>>;

  #[test]
  fn quux_attributes_keep_payloads_reachable() -> ScoredQuux {
    ensure_property(&any::<Quux>(), "mixed variant attributes keep payload scores reachable", |value| {
      let payload_score = value.payload_score();
      ensure_that(
        (value, payload_score),
        "variant attributes retain the expected payload and score",
        |observed| match observed.0 {
          Quux::Bare(payload) => observed.1 == payload,
          Quux::Pair(payload, ref text) => {
            let expected = payload.saturating_add(text.len());
            observed.1 == expected
          }
          Quux::PinnedPair(..) => observed.1 == 3,
          Quux::PinnedWord(_) => observed.1 == 1337,
          Quux::Braced {
            _foo: foo,
          } => (10..20).contains(&foo),
        },
      )
    })
  }

  #[test]
  fn asserting_arbitrary() {
    fn assert_arbitrary<T: Arbitrary>() {}

    assert_arbitrary::<Foo>();
    assert_arbitrary::<Custom>();
    assert_arbitrary::<Bobby>();
    assert_arbitrary::<Quux>();
  }
}
