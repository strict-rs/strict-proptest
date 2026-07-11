// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for the derive's type-parameter usage
//! tracking.
//!
//! Derives `Arbitrary` for a struct `Foo<V, T, U>` whose `U` appears only
//! inside a `PhantomData` field, then instantiates it with a `U` that is
//! not `Arbitrary`. Only type parameters used in real fields must receive
//! the generated `Arbitrary` bound, so the impl resolves without bounding
//! `U`. This exercises `src/use_tracking.rs`.

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use proptest::prelude::Arbitrary;
    use proptest_derive::Arbitrary;
    use strict_test_support::{TestFailure, ensure};

    #[derive(Debug)]
    struct NotArbitrary;

    #[derive(Debug, Arbitrary)]
    // Generic types are not in alphabetical order on purpose.
    struct Foo<V, T, U> {
        first: V,
        second: T,
        phantom: PhantomData<U>,
    }

    impl<V, T, U> Foo<V, T, U> {
        fn into_parts(self) -> (V, T) {
            (self.first, self.second)
        }
    }

    #[test]
    fn foo_fields_are_available_without_u_arbitrary_bound()
    -> Result<(), TestFailure> {
        let foo = Foo {
            first: 1,
            second: 2,
            phantom: PhantomData::<NotArbitrary>,
        };

        ensure(
            foo.into_parts() == (1, 2),
            "the non-phantom fields round-trip without a bound on U",
        )
    }

    #[test]
    fn asserting_arbitrary() {
        fn assert_arbitrary<T: Arbitrary>() {}

        assert_arbitrary::<Foo<i32, i32, NotArbitrary>>();
    }
}
