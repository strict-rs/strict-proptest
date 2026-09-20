//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::option`.

use core::convert::Infallible;
use core::ops::RangeInclusive;
use core::option as opt;

use crate::arbitrary::Arbitrary;
use crate::arbitrary::SMapped;
use crate::arbitrary::any_with;
use crate::option::OptionStrategy;
use crate::option::Probability;
use crate::option::weighted;
use crate::strategy::MapInto;
use crate::strategy::Strategy as _;
use crate::strategy::statics::static_map;

arbitrary!(Probability, MapInto<RangeInclusive<f64>, Self>;
    (0.0..=1.0).prop_map_into()
);

// `string::ParseError` aliases `Infallible`, which also aliases `!` on current
// nightly Rust. One implementation covers those names without overlapping.
arbitrary!(Option<Infallible>; None::<Infallible>);

arbitrary!([A: Arbitrary] Option<A>, OptionStrategy<A::Strategy>,
    product_type![Probability, A::Parameters];
    args => {
        let product_unpack![prob, elem_params] = args;
        weighted(prob, any_with::<A>(elem_params))
    }
);

lift1!([] Option<A>, Probability; base, prob => weighted(prob, base));

arbitrary!([A: Arbitrary] opt::IntoIter<A>, SMapped<Option<A>, Self>,
    <Option<A> as Arbitrary>::Parameters;
    args => static_map(any_with::<Option<A>>(args), Option::into_iter));

lift1!(['static] opt::IntoIter<A>, Probability;
    base, prob => weighted(prob, base).prop_map(Option::into_iter)
);

#[cfg(test)]
mod test {
  use super::*;
  use crate::std_facade::string;
  use crate::strategy::Just;
  use crate::test_runner::Reason;

  no_panic_test!(
      probability => Probability,
      option      => Option<u8>,
      option_iter => opt::IntoIter<u8>,
      option_parse_error => Option<string::ParseError>
  );

  #[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
  no_panic_test!(option_never => Option<!>);

  /// Retain the tree, its initial value, and the actual simplify result.
  type NoneObservation = Result<(Just<Option<Infallible>>, Option<Infallible>, bool), Reason>;

  #[test]
  fn option_infallible_always_generates_none() -> Result<(), strict_test_support::PredicateFailure<NoneObservation>> {
    use crate::arbitrary::any;
    use crate::strategy::Strategy as _;
    use crate::strategy::ValueTree as _;
    use crate::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let observation = any::<Option<Infallible>>().new_tree(&mut runner).map(|mut tree| {
      let value = tree.current();
      let simplified = tree.simplify();
      (tree, value, simplified)
    });
    strict_test_support::ensure_that(
      observation,
      "Option<Infallible> generates only None and cannot simplify",
      |result| {
        result
          .as_ref()
          .is_ok_and(|reached| reached.1.is_none() && reached.0.current().is_none() && !reached.2)
      },
    )
    .map(drop)
  }
}
