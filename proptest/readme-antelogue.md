
# Verification

The full local verification gate covers stable all-features,
stable no-default-features, stable `alt-stable` substitutes, and the nightly
exact `unstable` surface:

```sh
cargo +nightly fmt --all
cargo check --workspace --all-targets --all-features
cargo check --workspace --all-targets --no-default-features
cargo check -p proptest --no-default-features --features "alloc libm alt-stable"
cargo clippy --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --no-default-features
cargo clippy -p proptest --no-default-features --features "alloc libm alt-stable"
cargo test --workspace --all-targets --all-features
cargo test --workspace --all-targets --no-default-features
cargo test -p proptest --no-default-features --features "alloc libm alt-stable"
cargo +nightly check --workspace --all-targets --all-features
cargo +nightly check --workspace --all-targets --no-default-features
cargo +nightly check --workspace --all-targets --features proptest/unstable
cargo +nightly check -p proptest --no-default-features --features "alloc libm unstable"
cargo +nightly clippy --workspace --all-targets --all-features
cargo +nightly clippy --workspace --all-targets --no-default-features
cargo +nightly clippy --workspace --all-targets --features proptest/unstable
cargo +nightly clippy -p proptest --no-default-features --features "alloc libm unstable"
cargo +nightly test --workspace --all-targets --all-features
cargo +nightly test --workspace --all-targets --no-default-features
cargo +nightly test --workspace --all-targets --features proptest/unstable
cargo +nightly test -p proptest --no-default-features --features "alloc libm unstable"
```

# Acknowledgements

This crate wouldn't have come into existence had it not been for the [Rust port
of QuickCheck](https://github.com/BurntSushi/quickcheck) and the
[`regex_generate`](https://github.com/CryptArchy/regex_generate) crate which
gave wonderful examples of what is possible.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
