// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module provides integration tests that test the expansion
//! of the derive macro.

use crate::derive::impl_proptest_arbitrary;
use crate::util::PayloadFields;
use strict_test_support::{TestFailure, ensure, ensure_contains};
use syn::{ItemEnum, parse_quote};

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
// Enum payload normalization:
//==============================================================================

#[test]
fn unit_tuple_and_struct_zero_payload_variants_normalize_as_payloadless()
-> Result<(), TestFailure> {
    let item: ItemEnum = parse_quote! {
        enum ZeroPayloadVariants {
            Unit,
            Tuple(),
            Struct {},
            TuplePayload(u8),
            StructPayload { value: u8 },
        }
    };

    let payload_counts: Vec<_> = item
        .variants
        .into_iter()
        .map(|variant| PayloadFields::from(variant.fields).as_slice().len())
        .collect();

    ensure(
        payload_counts.as_slice() == [0, 0, 0, 1, 1],
        "unit, empty tuple, and empty struct variants normalize to zero payload fields",
    )
}

fn ensure_e0029_unit_variant_diagnostic(
    input: &str,
    attribute_fragment: &str,
) -> Result<(), TestFailure> {
    let parsed = strict_test_support::ensure_ok(
        syn::parse_str::<syn::DeriveInput>(input),
        "zero-payload variant diagnostic input parses as a derive input",
    )?;
    let output = format!("{}", impl_proptest_arbitrary(parsed));

    ensure_contains(
        &output,
        "compile_error",
        "the redundant unit-variant attribute emits compile_error tokens",
    )?;
    ensure_contains(
        &output,
        "[proptest_derive, E0029]",
        "the redundant unit-variant attribute emits E0029",
    )?;
    ensure_contains(
        &output,
        attribute_fragment,
        "the redundant unit-variant diagnostic names the attribute family",
    )?;
    ensure_contains(
        &output,
        "unit variant has no effect",
        "the redundant unit-variant diagnostic describes the no-payload path",
    )
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_params()
-> Result<(), TestFailure> {
    for input in [
        "
            #[derive(Debug)]
            enum UnitVariant {
                #[proptest(no_params)]
                Unit,
            }
        ",
        r#"
            #[derive(Debug)]
            enum EmptyTupleVariant {
                #[proptest(params = "u8")]
                Tuple(),
            }
        "#,
        "
            #[derive(Debug)]
            enum EmptyStructVariant {
                #[proptest(no_params)]
                Struct {},
            }
        ",
    ] {
        ensure_e0029_unit_variant_diagnostic(input, "params")?;
    }

    Ok(())
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_filter()
-> Result<(), TestFailure> {
    for input in [
        "
            #[derive(Debug)]
            enum UnitVariant {
                #[proptest(filter(foo))]
                Unit,
            }
        ",
        "
            #[derive(Debug)]
            enum EmptyTupleVariant {
                #[proptest(filter(foo))]
                Tuple(),
            }
        ",
        "
            #[derive(Debug)]
            enum EmptyStructVariant {
                #[proptest(filter(foo))]
                Struct {},
            }
        ",
    ] {
        ensure_e0029_unit_variant_diagnostic(input, "filter")?;
    }

    Ok(())
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_strategy()
-> Result<(), TestFailure> {
    for input in [
        r#"
            #[derive(Debug)]
            enum UnitVariant {
                #[proptest(strategy = "Just(UnitVariant::Unit)")]
                Unit,
            }
        "#,
        r#"
            #[derive(Debug)]
            enum EmptyTupleVariant {
                #[proptest(strategy = "Just(EmptyTupleVariant::Tuple)")]
                Tuple(),
            }
        "#,
        r#"
            #[derive(Debug)]
            enum EmptyStructVariant {
                #[proptest(strategy = "Just(EmptyStructVariant::Struct)")]
                Struct {},
            }
        "#,
    ] {
        ensure_e0029_unit_variant_diagnostic(input, "strategy")?;
    }

    Ok(())
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_value()
-> Result<(), TestFailure> {
    for input in [
        r#"
            #[derive(Debug)]
            enum UnitVariant {
                #[proptest(value = "UnitVariant::Unit")]
                Unit,
            }
        "#,
        r#"
            #[derive(Debug)]
            enum EmptyTupleVariant {
                #[proptest(value = "EmptyTupleVariant::Tuple")]
                Tuple(),
            }
        "#,
        r#"
            #[derive(Debug)]
            enum EmptyStructVariant {
                #[proptest(value = "EmptyStructVariant::Struct")]
                Struct {},
            }
        "#,
    ] {
        ensure_e0029_unit_variant_diagnostic(input, "value")?;
    }

    Ok(())
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_regex()
-> Result<(), TestFailure> {
    for input in [
        r#"
            #[derive(Debug)]
            enum UnitVariant {
                #[proptest(regex = "a+")]
                Unit,
            }
        "#,
        r#"
            #[derive(Debug)]
            enum EmptyTupleVariant {
                #[proptest(regex = "b*")]
                Tuple(),
            }
        "#,
        r#"
            #[derive(Debug)]
            enum EmptyStructVariant {
                #[proptest(regex = "a|b")]
                Struct {},
            }
        "#,
    ] {
        ensure_e0029_unit_variant_diagnostic(input, "regex")?;
    }

    Ok(())
}

test! {
    unit_variant_form_expands_to_unit_constructor {
        #[derive(Debug)]
        enum MyUnitVariant {
            Unit,
        }
    } expands to {
        impl ::proptest::arbitrary::Arbitrary for MyUnitVariant {
            type Parameters = ();
            type Strategy = fn() -> Self;

            fn arbitrary_with(_top: Self::Parameters) -> Self::Strategy {
                {
                    let () = _top;
                    { let value_fn: fn() -> _ = || MyUnitVariant::Unit {}; value_fn }
                }
            }
        }
    }
}

test! {
    empty_tuple_variant_form_expands_to_unit_constructor {
        #[derive(Debug)]
        enum MyEmptyTupleVariant {
            EmptyTuple(),
        }
    } expands to {
        impl ::proptest::arbitrary::Arbitrary for MyEmptyTupleVariant {
            type Parameters = ();
            type Strategy = fn() -> Self;

            fn arbitrary_with(_top: Self::Parameters) -> Self::Strategy {
                {
                    let () = _top;
                    {
                        let value_fn: fn() -> _ =
                            || MyEmptyTupleVariant::EmptyTuple {};
                        value_fn
                    }
                }
            }
        }
    }
}

test! {
    empty_struct_variant_form_expands_to_unit_constructor {
        #[derive(Debug)]
        enum MyEmptyStructVariant {
            EmptyStruct {},
        }
    } expands to {
        impl ::proptest::arbitrary::Arbitrary for MyEmptyStructVariant {
            type Parameters = ();
            type Strategy = fn() -> Self;

            fn arbitrary_with(_top: Self::Parameters) -> Self::Strategy {
                {
                    let () = _top;
                    {
                        let value_fn: fn() -> _ =
                            || MyEmptyStructVariant::EmptyStruct {};
                        value_fn
                    }
                }
            }
        }
    }
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
