use std::path::Path;

use strict_test_support::TestFailure;
use strict_test_support::ensure_snapshot;

use crate::property_test::codegen;
use crate::property_test::options::Options;

/// Native fixture parse and snapshot failures.
#[derive(Debug, thiserror::Error)]
enum SnapshotFailure {
  /// Invalid source or generated syntax.
  #[error(transparent)]
  Parse(#[from] syn::Error),
  /// Native snapshot failure.
  #[error(transparent)]
  Snapshot(#[from] TestFailure),
}

/// Expand real property syntax through the generator and compare its diagnostic artifact.
macro_rules! snapshot_test {
    ($name:ident {$($t:tt)*}) => {
        #[test]
        fn $name() -> Result<(), SnapshotFailure> {
            let input = syn::parse2(quote::quote! { $($t)* })?;
            let tokens = codegen::generate(input, &Options::default());
            let formatted = prettyplease::unparse(&syn::parse2(tokens)?);
            ensure_snapshot(&formatted,
                Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots/", stringify!($name), ".snap")),
                "custom strategy expansion preserves its typed callback").map_err(SnapshotFailure::Snapshot)
        }
    };
}

snapshot_test!(basic_derive_example {
    fn foo(x: i32, y: String) -> Result<(i32, String), Failure> {
        check((x, y))
    }
});

snapshot_test!(custom_strategy {
    fn foo(
        #[strategy = 123] x: i32,
        #[strategy = a + more()("complex") - expression!()] y: String,
    ) -> Result<(i32, String), Failure> {
        check((x, y))
    }
});

snapshot_test!(mix_custom_and_default_strategies {
    fn foo(
        x: i32,
        #[strategy = a + more()("complex") - expression!()] y: String,
    ) -> Result<(i32, String), Failure> {
        check((x, y))
    }
});
