// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Compile-and-run coverage for the `#[proptest(regex = ...)]` field
//! attribute.
//!
//! Derives `Arbitrary` for structs and enum variants whose `String`,
//! `Vec<u8>`, and custom `StrategyFromRegex` fields carry a `regex`
//! modifier in every spelling (`= "..."`, `(...)`, a `fn` path, and raw
//! strings), some combined with `filter`. Each generated field is checked
//! to match the regex it was given.

#[cfg(test)]
mod tests {
    use proptest::prelude::{Arbitrary, BoxedStrategy, Strategy as _, any};
    use proptest::strict::{TestResult, ensure_property};
    use proptest::string::StrategyFromRegex;
    use proptest_derive::Arbitrary;
    use strict_test_support::{ensure, ensure_ok};

    const fn mk_regex() -> &'static str {
        "[0-9][0-9]"
    }

    // struct:

    #[derive(Debug, Arbitrary)]
    struct T0 {
        #[proptest(regex = "a+")]
        foo: String,
        #[proptest(regex("b+"))]
        bar: String,
        #[proptest(regex(mk_regex))]
        baz: String,
        #[proptest(regex = "(a|b)+")]
        quux: Vec<u8>,
        #[proptest(regex("[abc]+"), filter("|bytes| bytes.len() < 4"))]
        wibble: Vec<u8>,
        #[proptest(regex(mk_regex))]
        wobble: Vec<u8>,
    }

    #[derive(Debug, Arbitrary)]
    struct T1(
        #[proptest(regex = "a+")] String,
        #[proptest(regex("b+"))] String,
        #[proptest(regex(mk_regex))] String,
        #[proptest(regex = "(a|b)+")] Vec<u8>,
        #[proptest(regex("[abc]+"), filter("|bytes| bytes.len() < 4"))] Vec<u8>,
        #[proptest(regex(mk_regex))] Vec<u8>,
    );

    #[derive(Debug, Arbitrary)]
    struct T1r(
        #[proptest(regex = r"\x61+")] String,
        #[proptest(regex(r"\x62+"))] String,
        #[proptest(regex(mk_regex))] String,
        #[proptest(regex = r"(\x61|\x62)+")] Vec<u8>,
        #[proptest(
            regex(r"[\x61\x62\x63]+"),
            filter("|bytes| bytes.len() < 4")
        )]
        Vec<u8>,
        #[proptest(regex(mk_regex))] Vec<u8>,
    );

    // enum:

    #[derive(Debug, Arbitrary)]
    enum T2 {
        V0 {
            #[proptest(regex = "a+")]
            foo: String,
            #[proptest(regex("b+"))]
            bar: String,
            #[proptest(regex(mk_regex))]
            baz: String,
            #[proptest(regex = "(a|b)+")]
            quux: Vec<u8>,
            #[proptest(regex("[abc]+"), filter("|bytes| bytes.len() < 4"))]
            wibble: Vec<u8>,
            #[proptest(regex(mk_regex))]
            wobble: Vec<u8>,
        },
    }

    #[derive(Debug, Arbitrary)]
    enum T3 {
        V0(
            #[proptest(regex = "a+")] String,
            #[proptest(regex("b+"))] String,
            #[proptest(regex(mk_regex))] String,
            #[proptest(regex = "(a|b)+")] Vec<u8>,
            #[proptest(regex("[abc]+"), filter("|bytes| bytes.len() < 4"))]
            Vec<u8>,
            #[proptest(regex(mk_regex))] Vec<u8>,
        ),
    }

    // Show that it works for new types and that `String` | `Vec<u8>` isn't
    // hardcoded into the logic:

    #[derive(Debug)]
    struct NewString(String);

    impl StrategyFromRegex for NewString {
        type Strategy = BoxedStrategy<Self>;

        fn from_regex(regex: &str) -> Self::Strategy {
            String::from_regex(regex).prop_map(NewString).boxed()
        }
    }

    #[derive(Debug, Arbitrary)]
    struct T4(#[proptest(regex = "a+")] NewString);

    fn ensure_aplus(x0: &str) -> TestResult {
        ensure(x0.chars().count() > 0, "the a+ string is non-empty")?;
        ensure(
            x0.chars().all(|letter: char| letter == 'a'),
            "the a+ string is all a's",
        )
    }

    fn ensure_adherence(
        x0: &str,
        x1: &str,
        x2: &str,
        y0: &[u8],
        y1: &[u8],
        y2: &[u8],
    ) -> TestResult {
        ensure_aplus(x0)?;

        ensure(x1.chars().count() > 0, "the b+ string is non-empty")?;
        ensure(
            x1.chars().all(|letter: char| letter == 'b'),
            "the b+ string is all b's",
        )?;

        let parsed = ensure_ok(
            x2.parse::<u8>(),
            "the two-digit regex string parses as u8",
        )?;
        ensure(parsed < 100, "the two-digit value stays below one hundred")?;

        ensure(!y0.is_empty(), "the (a|b)+ bytes are non-empty")?;
        ensure(
            y0.iter().all(|byte: &u8| b"ab".contains(byte)),
            "the (a|b)+ bytes stay in the alphabet",
        )?;

        ensure(
            !y1.is_empty() && y1.len() < 4,
            "the filtered [abc]+ bytes keep the length filter",
        )?;
        ensure(
            y1.iter().all(|byte: &u8| b"abc".contains(byte)),
            "the [abc]+ bytes stay in the alphabet",
        )?;

        ensure(!y2.is_empty(), "the fn-regex bytes are non-empty")?;
        ensure(
            y2.iter().all(u8::is_ascii_digit),
            "the fn-regex bytes are all digits",
        )
    }

    #[test]
    fn t0_adhering_to_regex() -> TestResult {
        ensure_property(
            &any::<T0>(),
            "named struct regex fields adhere to their regexes",
            |sample| {
                let T0 {
                    foo: x0,
                    bar: x1,
                    baz: x2,
                    quux: y0,
                    wibble: y1,
                    wobble: y2,
                } = sample;
                ensure_adherence(&x0, &x1, &x2, &y0, &y1, &y2)
            },
        )
    }

    #[test]
    fn t1_adhering_to_regex() -> TestResult {
        ensure_property(
            &any::<T1>(),
            "tuple struct regex fields adhere to their regexes",
            |sample| {
                let T1(x0, x1, x2, y0, y1, y2) = sample;
                ensure_adherence(&x0, &x1, &x2, &y0, &y1, &y2)
            },
        )
    }

    #[test]
    fn t1_r_adhering_to_regex() -> TestResult {
        ensure_property(
            &any::<T1r>(),
            "raw-string regex fields adhere to their regexes",
            |sample| {
                let T1r(x0, x1, x2, y0, y1, y2) = sample;
                ensure_adherence(&x0, &x1, &x2, &y0, &y1, &y2)
            },
        )
    }

    #[test]
    fn t2_adhering_to_regex() -> TestResult {
        ensure_property(
            &any::<T2>(),
            "struct-variant regex fields adhere to their regexes",
            |sample| {
                let T2::V0 {
                    foo: x0,
                    bar: x1,
                    baz: x2,
                    quux: y0,
                    wibble: y1,
                    wobble: y2,
                } = sample;
                ensure_adherence(&x0, &x1, &x2, &y0, &y1, &y2)
            },
        )
    }

    #[test]
    fn t3_adhering_to_regex() -> TestResult {
        ensure_property(
            &any::<T3>(),
            "tuple-variant regex fields adhere to their regexes",
            |sample| {
                let T3::V0(x0, x1, x2, y0, y1, y2) = sample;
                ensure_adherence(&x0, &x1, &x2, &y0, &y1, &y2)
            },
        )
    }

    #[test]
    fn t4_adhering_to_regex() -> TestResult {
        ensure_property(
            &any::<T4>(),
            "a custom StrategyFromRegex type adheres to its regex",
            |sample| ensure_aplus(&(sample.0).0),
        )
    }

    #[test]
    fn asserting_arbitrary() {
        fn assert_arbitrary<T: Arbitrary>() {}

        assert_arbitrary::<T0>();
        assert_arbitrary::<T1>();
        assert_arbitrary::<T2>();
        assert_arbitrary::<T3>();
        assert_arbitrary::<T4>();
    }
}
