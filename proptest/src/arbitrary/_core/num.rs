//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::num`.

use core::num::FpCategory;
use core::num::ParseFloatError;
use core::num::ParseIntError;
use core::num::Saturating;
#[cfg(feature = "unstable")]
use core::num::TryFromIntError;
use core::num::Wrapping;

use crate::strategy::Just;
use crate::strategy::TupleUnion;
use crate::strategy::WeightedStrategy;

arbitrary!(ParseFloatError; {
    loop {
        if let Err(error) = "".parse::<f32>() {
            break error;
        }
    }
});
arbitrary!(ParseIntError; {
    loop {
        if let Err(error) = "".parse::<u32>() {
            break error;
        }
    }
});

#[cfg(feature = "unstable")]
arbitrary!(TryFromIntError; {
    use core::convert::TryFrom as _;
    loop {
        if let Err(error) = u8::try_from(-1) {
            break error;
        }
    }
});

wrap_ctor!(Wrapping, Wrapping);

wrap_ctor!(Saturating, Saturating);

arbitrary!(FpCategory,
    TupleUnion<(WeightedStrategy<Just<Self>>, WeightedStrategy<Just<Self>>, WeightedStrategy<Just<Self>>,
                WeightedStrategy<Just<Self>>, WeightedStrategy<Just<Self>>)>;
    {
        use core::num::FpCategory::{
            Infinite, Nan, Normal, Subnormal, Zero,
        };
        prop_oneof![
            Just(Nan),
            Just(Infinite),
            Just(Zero),
            Just(Subnormal),
            Just(Normal),
        ]
    }
);

#[cfg(test)]
mod test {
  use super::*;

  no_panic_test!(
      parse_float_error => ParseFloatError,
      parse_int_error => ParseIntError,
      wrapping => Wrapping<u8>,
      saturating => Saturating<u8>,
      fp_category => FpCategory
  );

  #[cfg(feature = "unstable")]
  no_panic_test!(
      try_from_int_error => TryFromIntError
  );
}
