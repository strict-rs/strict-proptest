// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module provides integration tests that test the expansion
//! of the derive macro.

//==============================================================================
// Macros:
//==============================================================================

// Borrowed from:
// https://docs.rs/synstructure/0.7.0/src/synstructure/macros.rs.html#104-135,
// reshaped so the comparison flows as `Result<(), TestFailure>` instead of
// panicking.
macro_rules! test_derive {
    ($name:path { $($i:tt)* } expands to { $($o:tt)* }) => {
        {
            let expected = ::quote::quote!($($o)*);

            let i = stringify!( $($i)* );
            let parsed = ::strict_test_support::ensure_ok(
                ::syn::parse_str::<::syn::DeriveInput>(i),
                concat!("Failed to parse input to `#[derive(",
                    stringify!($name),
                ")]`"),
            )?;
            let res = $name(parsed);
            ::strict_test_support::ensure_eq(
                &format!("{}", res),
                &format!("{}", expected),
                "the derive expansion matches the pinned tokens",
            )
        }
    };
}

macro_rules! test {
    ($test_name:ident { $($i:tt)* } expands to { $($o:tt)* }) => {
        #[test]
        fn $test_name(
        ) -> ::core::result::Result<(), ::strict_test_support::TestFailure>
        {
            test_derive!(
                $crate::derive::impl_proptest_arbitrary { $($i)* }
                expands to { $($o)* }
            )
        }
    };
}

//==============================================================================
// Unit structs:
//==============================================================================

test! {
    struct_unit_unit {
        #[derive(Debug)]
        struct MyUnitStruct;
    } expands to {
        impl ::proptest::arbitrary::Arbitrary for MyUnitStruct {
            type Parameters = ();
            type Strategy = fn() -> Self;

            fn arbitrary_with(_top: Self::Parameters) -> Self::Strategy {
                { let value_fn: fn() -> _ = || MyUnitStruct {}; value_fn }
            }
        }
    }
}

test! {
    struct_unit_tuple {
        #[derive(Debug)]
        struct MyTupleUnitStruct();
    } expands to {
        impl ::proptest::arbitrary::Arbitrary for MyTupleUnitStruct {
            type Parameters = ();
            type Strategy = fn() -> Self;

            fn arbitrary_with(_top: Self::Parameters) -> Self::Strategy {
                { let value_fn: fn() -> _ = || MyTupleUnitStruct {}; value_fn }
            }
        }
    }
}

test! {
    struct_unit_named {
        #[derive(Debug)]
        struct MyNamedUnitStruct {}
    } expands to {
        impl ::proptest::arbitrary::Arbitrary for MyNamedUnitStruct {
            type Parameters = ();
            type Strategy = fn() -> Self;

            fn arbitrary_with(_top: Self::Parameters) -> Self::Strategy {
                { let value_fn: fn() -> _ = || MyNamedUnitStruct {}; value_fn }
            }
        }
    }
}

test! {
    associated_projection_bounds_are_deduplicated {
        #[derive(Debug)]
        struct AssociatedTwice<T: Iterator> {
            first: T::Item,
            second: T::Item,
        }
    } expands to {
        impl<T: Iterator + ::std::fmt::Debug>
            ::proptest::arbitrary::Arbitrary for AssociatedTwice<T>
        where
            T::Item: ::proptest::arbitrary::Arbitrary
        {
            type Parameters = (
                <T::Item as ::proptest::arbitrary::Arbitrary> :: Parameters,
                <T::Item as ::proptest::arbitrary::Arbitrary> :: Parameters,
            );

            type Strategy = ::proptest::strategy::Map<
                (
                    <T::Item as ::proptest::arbitrary::Arbitrary> :: Strategy,
                    <T::Item as ::proptest::arbitrary::Arbitrary> :: Strategy,
                ),
                fn((T::Item, T::Item,)) -> Self
            >;

            fn arbitrary_with(_top: Self::Parameters) -> Self::Strategy {
                {
                    let (param_0, param_1,) = _top;
                    ::proptest::strategy::Strategy::prop_map(
                        (
                            ::proptest::arbitrary::any_with :: <T::Item>(param_0),
                            ::proptest::arbitrary::any_with :: <T::Item>(param_1),
                        ),
                        |(tmp_0, tmp_1,)| AssociatedTwice {
                            first: tmp_0,
                            second: tmp_1
                        }
                    )
                }
            }
        }
    }
}
