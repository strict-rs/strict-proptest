use strict_test_support::{TestFailure, ensure_ok};
use syn::parse_quote;

use crate::property_test::{codegen, options::Options};

/// Helper macro to make snapshot tests
///
/// The code inside the block is parsed as a function and the `#[property_test]` macro is applied
/// to it. The generated code is then formatted, and passed to the snapshot testing library
///
/// If `fails` is supplied, then the output will be
macro_rules! snapshot_test {
    ($name:ident {$($t:tt)*}) => {
        #[test]
        fn $name() -> Result<(), TestFailure> {
            let input = parse_quote! { $($t)* };
            let tokens = codegen::generate(input, Options::default());
            let file = ensure_ok(
                syn::parse_file(&tokens.to_string()),
                "generated code parses as a file",
            )?;
            let formatted = prettyplease::unparse(&file);

            insta::assert_snapshot!(formatted);
            Ok(())
        }
    };
}

snapshot_test!(basic_derive_example {
    fn foo(x: i32, y: String) -> ::proptest::strict::TestResult {
        let _ = (x, y);
        Ok(())
    }
});

snapshot_test!(custom_strategy {
    fn foo(
        #[strategy = 123] x: i32,
        #[strategy = a + more()("complex") - expression!()] y: String,
    ) -> ::proptest::strict::TestResult {
        let _ = (x, y);
        Ok(())
    }
});

snapshot_test!(mix_custom_and_default_strategies {
    fn foo(
        x: i32,
        #[strategy = a + more()("complex") - expression!()] y: String,
    ) -> ::proptest::strict::TestResult {
        let _ = (x, y);
        Ok(())
    }
});
