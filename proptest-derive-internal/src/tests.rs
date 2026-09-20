// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module provides integration tests that test the expansion
//! of the derive macro.

use strict_test_support::ComparisonFailure;
use strict_test_support::PredicateFailure;
use strict_test_support::ensure_eq;
use strict_test_support::ensure_that;
use syn::ItemEnum;

/// Native parser, expansion, and diagnostic assertion failures.
#[derive(Debug, thiserror::Error)]
enum ExpansionFailure {
  /// Invalid fixture or generated syntax.
  #[error(transparent)]
  Parse(#[from] syn::Error),
  /// Complete generated and expected syntax trees.
  #[error(transparent)]
  Expansion(Box<ComparisonFailure<syn::File, syn::File>>),
  /// Native variant payload counts.
  #[error(transparent)]
  Counts(#[from] ComparisonFailure<Vec<usize>, [usize; 5]>),
  /// Complete compiler diagnostics for all unit-variant forms.
  #[error(transparent)]
  Diagnostics(#[from] PredicateFailure<Vec<String>>),
}

use crate::derive::impl_proptest_arbitrary;
use crate::util::PayloadFields;

//==============================================================================
// Macros:
//==============================================================================

/// Compare native parsed expansion trees, preserving both complete subjects.
macro_rules! test {
  ($test_name:ident { $($input:tt)* } expands to { $($expected:tt)* }) => {
    #[test]
    fn $test_name() -> Result<(), ExpansionFailure> {
      let parsed = syn::parse2(quote::quote!($($input)*))?;
      let actual: syn::File = syn::parse2(crate::derive::impl_proptest_arbitrary(parsed))?;
      let expected: syn::File = syn::parse2(quote::quote!($($expected)*))?;
      ensure_eq(actual, expected, "the derive expansion matches its complete syntax contract")
        .map(drop).map_err(|failure| ExpansionFailure::Expansion(Box::new(failure)))
    }
  };
}

//==============================================================================
// Enum payload normalization:
//==============================================================================

#[test]
fn unit_tuple_and_struct_zero_payload_variants_normalize_as_payloadless() -> Result<(), ExpansionFailure> {
  let item: ItemEnum = syn::parse2(quote::quote! {
      enum ZeroPayloadVariants {
          Unit,
          Tuple(),
          Struct {},
          TuplePayload(u8),
          StructPayload { value: u8 },
      }
  })?;

  let payload_counts: Vec<_> = item
    .variants
    .into_iter()
    .map(|variant| PayloadFields::from(variant.fields).as_slice().len())
    .collect();

  ensure_eq(
    payload_counts,
    [0, 0, 0, 1, 1],
    "unit, empty tuple, and empty struct variants normalize to zero payload fields",
  )
  .map(drop)
  .map_err(ExpansionFailure::Counts)
}

/// Retain the complete diagnostics of each redundant unit-variant attribute.
fn ensure_unit_diagnostics(inputs: &[&str; 3], attribute: &str) -> Result<Vec<String>, ExpansionFailure> {
  let diagnostics = inputs
    .iter()
    .map(|input| Ok(impl_proptest_arbitrary(syn::parse_str(input)?).to_string()))
    .collect::<Result<Vec<_>, syn::Error>>()?;
  ensure_that(
    diagnostics,
    "all zero-payload forms reject the redundant attribute with E0029",
    |observed| {
      observed.iter().all(|output| {
        output.contains("compile_error")
          && output.contains("[proptest_derive, E0029]")
          && output.contains(attribute)
          && output.contains("unit variant has no effect")
      })
    },
  )
  .map_err(ExpansionFailure::Diagnostics)
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_params() -> Result<(), ExpansionFailure> {
  let inputs = [
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
  ];
  ensure_unit_diagnostics(&inputs, "params").map(drop)
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_filter() -> Result<(), ExpansionFailure> {
  let inputs = [
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
  ];
  ensure_unit_diagnostics(&inputs, "filter").map(drop)
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_strategy() -> Result<(), ExpansionFailure> {
  let inputs = [
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
  ];
  ensure_unit_diagnostics(&inputs, "strategy").map(drop)
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_value() -> Result<(), ExpansionFailure> {
  let inputs = [
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
  ];
  ensure_unit_diagnostics(&inputs, "value").map(drop)
}

#[test]
fn unit_tuple_and_struct_zero_payload_variants_reject_redundant_regex() -> Result<(), ExpansionFailure> {
  let inputs = [
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
  ];
  ensure_unit_diagnostics(&inputs, "regex").map(drop)
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
