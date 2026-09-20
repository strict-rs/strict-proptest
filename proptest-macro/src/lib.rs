//! Procedural macros for the `proptest` crate.
//!
//! The crate currently provides one macro: [`macro@property_test`], an
//! attribute that rewrites an annotated test function into a strict property
//! test. Generated wrappers return native `PropertyResult` evidence and drive
//! `Result<A, E>` bodies through `proptest::strict`.

use proc_macro::TokenStream;

/// Token streams for one `#[property_test]` expansion.
struct PropertyTestInput {
  /// The attribute body supplied to `#[property_test(...)]`.
  attr:         proc_macro2::TokenStream,
  /// The function annotated with `#[property_test]`.
  annotated_fn: proc_macro2::TokenStream,
}

/// Expands a parsed `#[property_test]` invocation.
trait ExpandPropertyTest {
  /// Rewrite the annotated function into the generated strict property test.
  fn expand(self) -> proc_macro2::TokenStream;
}

/// The parse / validate / options / codegen pipeline that rewrites an
/// annotated function into the generated strict property test.
mod property_test;

/// The `property_test` attribute generates inputs and executes a typed property.
///
/// The annotated function declares `Result<A, E>` or an alias for that result.
/// Each callback returns its original successful evidence or concrete assertion
/// failure. The generated `#[test]` wrapper returns
/// `proptest::test_runner::PropertyResult<(ArgumentTypes, ...), A, E>`.
/// Its minimized counterexample is an ordinary argument tuple; failures and
/// intermediate successful evidence remain available without string conversion.
/// A unit-returning body is rejected at compile time.
///
/// # Example
///
/// ```rust,test_harness
/// use proptest_macro::property_test;
/// use strict_test_support::{ComparisonFailure, ensure_eq};
///
/// type ReversalCheck = Result<(Vec<u8>, Vec<u8>), ComparisonFailure<Vec<u8>, Vec<u8>>>;
///
/// #[property_test]
/// fn reversing_twice(v: Vec<u8>) -> ReversalCheck {
///     let mut reversed = v.clone();
///     reversed.reverse();
///     reversed.reverse();
///     ensure_eq(reversed, v, "double reversal restores the original")
/// }
/// ```
///
/// The body retains both vectors. The wrapper generates `(Vec<u8>,)` inputs,
/// attaches the argument label `v`, and returns the typed run report. In-process
/// assertion evidence need not implement `Clone`, `Send`, `Sync`, or a
/// serialization trait. Values remain alive until their report is consumed.
///
/// # Configuration
///
/// `config = <expr>` supplies the runner's configuration. The wrapper sets
/// `test_name` and `source_file` to the annotated function, preserving all other
/// fields, including cases, shrinking budgets, seed, and explicit persistence.
/// Without `config`, `strict_default_config()` preserves ordinary `PROPTEST_*`
/// settings, disables regression-file persistence, and resolves `STRICT_TEST_SEED`
/// as the fixed default `0x5EED`, an explicit integer, or `random`.
///
/// ```rust,test_harness
/// use proptest_macro::property_test;
/// use proptest::strict::strict_default_config;
/// use proptest::test_runner::Config;
/// use strict_test_support::{PredicateFailure, ensure_that};
///
/// #[property_test(config = Config { cases: 100, ..strict_default_config() })]
/// fn generated_digits(#[strategy = "[0-9]*"] value: String) -> Result<String, PredicateFailure<String>> {
///     ensure_that(value, "the digit strategy generates ASCII digits", |text| {
///         text.bytes().all(|byte| byte.is_ascii_digit())
///     })
/// }
/// ```
///
/// `proptest_path = ::path::to::proptest` selects a renamed or re-exported crate.
/// The strategy, configuration, typed return signature, and runner all use that
/// path.
///
/// `transport = CodecType => codec_expression` declares the type and constructor
/// of an explicit `PropertyTransport<(ArgumentTypes, ...), A, E>`. The constructor
/// is evaluated once per wrapper invocation. With this option the wrapper calls
/// `ensure_property_with_transport`, retaining the codec's concrete error type.
/// The codec must preserve the argument tuple, success and failure evidence, and
/// relevant replay state. Fork or timeout settings without a codec return an
/// explicit configuration failure before any property callback.
///
/// # Custom strategies and patterns
///
/// Each parameter uses its type's `Arbitrary` strategy unless it has one
/// `#[strategy = <expr>]` attribute. Multiple strategies on one parameter are a
/// compile error. Destructuring patterns and `mut` bindings are preserved in the
/// concretely typed callback, including method calls that require type inference.
///
/// # Generated API contract
///
/// The wrapper keeps the annotated name and visibility, takes no arguments, and
/// returns the native typed property result. Counterexample tuples follow the
/// original parameter order; zero parameters produce `()`. Diagnostic argument
/// labels describe those same parameters. No private generated parameter struct
/// must be named to inspect a counterexample. Internal expansion locals are not
/// part of the public API.
#[proc_macro_attribute]
#[allow(
  clippy::single_call_fn,
  reason = "proc_macro_attribute entry point that bridges proc_macro tokens into the internal pipeline"
)]
pub fn property_test(attr: TokenStream, annotated_fn: TokenStream) -> TokenStream {
  PropertyTestInput {
    attr:         attr.into(),
    annotated_fn: annotated_fn.into(),
  }
  .expand()
  .into()
}
