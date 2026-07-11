use proc_macro2::TokenStream;
use syn::parse2;

use crate::{ExpandPropertyTest, PropertyTestInput};

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

impl ExpandPropertyTest for PropertyTestInput {
    fn expand(self) -> TokenStream {
        let mut item_fn = parse!(self.annotated_fn);
        let options = parse!(self.attr);

        if let Err(compile_error) = validate(&mut item_fn) {
            return compile_error;
        }

        codegen::generate(item_fn, &options)
    }
}
