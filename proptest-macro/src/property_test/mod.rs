use proc_macro2::TokenStream;
use syn::parse2;

use self::validate::validate;

/// Rewrites the validated fn into a params struct, its `Arbitrary` impl, and
/// a tail call into the strict runner.
mod codegen;
/// Parses the attribute body (`config`, `proptest_path`) into `Options`.
mod options;
/// The bridge from a validated fn to codegen: `Argument`, `strip_args`, and
/// the `#[strategy = ...]` predicate.
mod utils;
/// Syntactic sanity checks on the annotated fn before code generation.
mod validate;

#[cfg(test)]
mod tests;

/// try to parse an item, or return the error as a token stream
macro_rules! parse {
    ($e:expr) => {
        match parse2($e) {
            Ok(parsed) => parsed,
            Err(error) => return error.into_compile_error(),
        }
    };
}

/// Rewrite an annotated function into a strict property test.
///
/// Runs the four-step pipeline: parse the annotated fn and the attribute
/// body, validate the signature, then hand off to `codegen::generate`. A
/// parse or validation failure is returned as `compile_error!` tokens rather
/// than a panic, so it surfaces as a normal spanned error at the use site.
#[allow(
    clippy::single_call_fn,
    reason = "drive the parse, validate, and codegen pipeline for one annotated fn"
)]
pub(crate) fn property_test(
    attr: TokenStream,
    annotated_fn: TokenStream,
) -> TokenStream {
    let mut item_fn = parse!(annotated_fn);
    let options = parse!(attr);

    if let Err(compile_error) = validate(&mut item_fn) {
        return compile_error;
    }

    codegen::generate(item_fn, options)
}
