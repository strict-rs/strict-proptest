//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::option`.

#[cfg(feature = "alt-stable")]
use core::convert::Infallible;
use core::ops::RangeInclusive;
use core::option as opt;

use crate::arbitrary::Arbitrary;
use crate::arbitrary::SMapped;
use crate::arbitrary::any_with;
use crate::option::OptionStrategy;
use crate::option::Probability;
use crate::option::weighted;
#[cfg(not(feature = "alt-stable"))]
use crate::std_facade::string;
use crate::strategy::MapInto;
use crate::strategy::Strategy as _;
use crate::strategy::statics::static_map;

arbitrary!(Probability, MapInto<RangeInclusive<f64>, Self>;
    (0.0..=1.0).prop_map_into()
);

// These are Option<AnUninhabitedType> impls:
#[cfg(not(feature = "alt-stable"))]
arbitrary!(Option<string::ParseError>; None::<string::ParseError>);
#[cfg(feature = "alt-stable")]
arbitrary!(Option<Infallible>; None::<Infallible>);
#[cfg(all(feature = "unstable", not(feature = "alt-stable")))]
arbitrary!(Option<!>; None);

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

  no_panic_test!(
      probability => Probability,
      option      => Option<u8>,
      option_iter => opt::IntoIter<u8>,
      option_parse_error => Option<string::ParseError>
  );

  #[cfg(feature = "alt-stable")]
  #[test]
  fn option_infallible_always_generates_none() -> Result<(), strict_test_support::TestFailure> {
    use crate::arbitrary::any;
    use crate::strategy::Strategy as _;
    use crate::strategy::ValueTree as _;
    use crate::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let mut tree = strict_test_support::ensure_some(
      any::<Option<Infallible>>().new_tree(&mut runner).ok(),
      "Option<Infallible> generates a value tree",
    )?;
    strict_test_support::ensure(tree.current().is_none(), "Option<Infallible> always generates None")?;
    strict_test_support::ensure(!tree.simplify(), "a None-only option strategy has no simpler value")
  }
}
