// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for the derive's `PhantomData` field
//! detection.
//!
//! Derives `Arbitrary` for structs carrying a `PhantomData<T>` field
//! written through every import spelling (`::std::marker::PhantomData`,
//! `marker::PhantomData`, bare `PhantomData`, and `std::marker::`), then
//! instantiates each with a phantom type that is not `Arbitrary`. The
//! phantom parameter must not receive the generated `Arbitrary` bound.

#[cfg(test)]
mod tests {
  mod support;

  use std::marker::PhantomData;

  use proptest_derive::Arbitrary;
  use support::assert_arbitrary;

  #[derive(Debug)]
  struct NotArbitrary;

  #[derive(Debug, Arbitrary)]
  struct T1<T>(PhantomData<T>);

  #[derive(Debug, Arbitrary)]
  struct T2(T1<NotArbitrary>);

  #[derive(Debug, Arbitrary)]
  struct T3<T>(PhantomData<T>);

  #[derive(Debug, Arbitrary)]
  struct T4(T3<NotArbitrary>);

  #[derive(Debug, Arbitrary)]
  struct T5<T>(PhantomData<T>);

  #[derive(Debug, Arbitrary)]
  struct T6(T5<NotArbitrary>);

  #[derive(Debug, Arbitrary)]
  struct T7<T>(PhantomData<T>);

  #[derive(Debug, Arbitrary)]
  struct T8(T7<NotArbitrary>);

  #[derive(Debug, Arbitrary)]
  struct T9<A, B, C> {
    _a: A,
    _b: B,
    _c: PhantomData<C>,
  }

  assert_arbitrary!(
    T1<NotArbitrary>,
    T2,
    T3<NotArbitrary>,
    T4,
    T5<NotArbitrary>,
    T6,
    T7<NotArbitrary>,
    T8,
    T9<u8, usize, NotArbitrary>,
  );
}
