# proptest-derive-internal

Implementation detail of [`proptest-derive`](https://crates.io/crates/proptest-derive):
the parsing, attribute interpretation, bound inference, and code generation
behind `#[derive(Arbitrary)]`, split out so the pipeline can be exercised as
ordinary library code.

This crate has no stable API and is not meant to be depended on directly. Use
`proptest-derive` instead.
