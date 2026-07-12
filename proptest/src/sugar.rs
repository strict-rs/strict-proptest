//-
// Copyright 2017, 2019 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::fmt;
#[cfg(feature = "std")]
use crate::test_runner::Config;
#[cfg(feature = "std")]
use crate::test_runner::emit_closure_fork_unsupported;

/// Easily define `proptest` tests.
///
/// Within `proptest!`, define one or more functions without return type
/// normally, except instead of putting `: type` after each parameter, write
/// `in strategy`, where `strategy` is an expression evaluating to some
/// `Strategy`.
///
/// Each function will be wrapped in a function which sets up a `TestRunner`,
/// and then invokes the function body with inputs generated according to the
/// strategies.
///
/// ### Example
///
/// ```
/// use proptest::prelude::*;
///
/// proptest! {
///   # /*
///   #[test]
///   # */
///   fn test_addition(a in 0..10, b in 0..10) {
///     prop_assert!(a + b <= 18);
///   }
///
///   # /*
///   #[test]
///   # */
///   fn test_string_concat(a in ".*", b in ".*") {
///     let cat = format!("{}{}", a, b);
///     prop_assert_eq!(a.len() + b.len(), cat.len());
///   }
/// }
/// #
/// # fn main() { test_addition(); test_string_concat(); }
/// ```
///
/// You can also use the normal argument syntax `pattern: type` as in:
///
/// ```rust
/// use proptest::prelude::*;
///
/// proptest! {
///   # /*
///   #[test]
///   # */
///   fn addition_is_commutative(a: u8, b: u8) {
///     prop_assert_eq!(a as u16 + b as u16, b as u16 + a as u16);
///   }
///
///   # /*
///   #[test]
///   # */
///   fn test_string_concat(a in ".*", b: String) {
///     let cat = format!("{}{}", a, b);
///     prop_assert_eq!(a.len() + b.len(), cat.len());
///   }
/// }
/// #
/// # fn main() { addition_is_commutative(); test_string_concat(); }
/// ```
///
/// As you can see, you can mix `pattern: type` and `pattern in expr`.
/// Due to limitations in `macro_rules!`, `pattern: type` does not work in
/// all circumstances. In such a case, use `(pattern): type` instead.
///
/// To override the default configuration, you can start the `proptest!` block
/// with `#![proptest_config(expr)]`, where `expr` is an expression that
/// evaluates to a `proptest::test_runner::Config` (or a reference to one).
///
/// ```
/// use proptest::prelude::*;
///
/// proptest! {
///   #![proptest_config(ProptestConfig {
///     cases: 99, .. ProptestConfig::default()
///   })]
///   # /*
///   #[test]
///   # */
///   fn test_addition(a in 0..10, b in 0..10) {
///     prop_assert!(a + b <= 18);
///   }
/// }
/// #
/// # fn main() { test_addition(); }
/// ```
///
/// ## Closure-Style Invocation
///
/// As of proptest 0.8.1, an alternative, "closure-style" invocation is
/// supported. In this form, `proptest!` is a function-like macro taking a
/// closure-esque argument. This makes it possible to run multiple tests that
/// require some expensive setup process. Note that the "fork" and "timeout"
/// features are _not_ supported in closure style.
///
/// To use a custom configuration, pass the `Config` object as a first
/// argument.
///
/// ### Example
///
/// ```
/// use proptest::prelude::*;
///
/// #[derive(Debug)]
/// struct BigStruct { /* Lots of fields ... */ }
///
/// fn very_expensive_function() -> BigStruct {
///   // Lots of code...
///   BigStruct { /* fields */ }
/// }
///
/// # /*
/// #[test]
/// # */
/// fn my_test() {
///   // We create just one `BigStruct`
///   let big_struct = very_expensive_function();
///
///   // But now can run multiple tests without needing to build it every time.
///   // Note the extra parentheses around the arguments are currently
///   // required.
///   proptest!(|(x in 0_u32..42_u32, y in 1000_u32..100000_u32)| {
///     // Test stuff
///   });
///
///   // `move` closures are also supported
///   proptest!(move |(x in 0_u32..42_u32)| {
///     // Test other stuff
///   });
///
///   // You can pass a custom configuration as the first argument
///   proptest!(ProptestConfig::with_cases(1000), |(x: i32)| {
///     // Test more stuff
///   });
/// }
/// #
/// # fn main() { my_test(); }
/// ```
#[macro_export]
macro_rules! proptest {
    ($($tokens:tt)*) => {
        $crate::__proptest_internal! { $($tokens)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __proptest_internal {
    (#![proptest_config($config:expr)]
     $(
        $(#[$meta:meta])*
       fn $test_name:ident($($parm:pat in $strategy:expr),+ $(,)?) $body:block
    )*) => {
        $(
            $(#[$meta])*
            fn $test_name()
                -> ::core::result::Result<
                    (),
                    $crate::std_facade::Box<dyn ::core::fmt::Debug>,
                >
            {
                let mut config = $crate::test_runner::contextualize_config($config.clone());
                config.test_name = ::core::option::Option::Some(
                    ::core::concat!(::core::module_path!(), "::", ::core::stringify!($test_name)));
                $crate::proptest_helper!(@_BODY config ($($parm in $strategy),+) [] $body)
            }
        )*
    };
    (#![proptest_config($config:expr)]
     $(
        $(#[$meta:meta])*
        fn $test_name:ident($($arg:tt)+) $body:block
    )*) => {
        $(
            $(#[$meta])*
            fn $test_name()
                -> ::core::result::Result<
                    (),
                    $crate::std_facade::Box<dyn ::core::fmt::Debug>,
                >
            {
                let mut config = $crate::test_runner::contextualize_config($config.clone());
                config.test_name = ::core::option::Option::Some(
                    ::core::concat!(::core::module_path!(), "::", ::core::stringify!($test_name)));
                $crate::proptest_helper!(@_BODY2 config ($($arg)+) [] $body)
            }
        )*
    };

    ($(
        $(#[$meta:meta])*
        fn $test_name:ident($($parm:pat in $strategy:expr),+ $(,)?) $body:block
    )*) => { $crate::__proptest_internal! {
        #![proptest_config($crate::test_runner::Config::default())]
        $($(#[$meta])*
          fn $test_name($($parm in $strategy),+) $body)*
    } };

    ($(
        $(#[$meta:meta])*
        fn $test_name:ident($($arg:tt)+) $body:block
    )*) => { $crate::__proptest_internal! {
        #![proptest_config($crate::test_runner::Config::default())]
        $($(#[$meta])*
          fn $test_name($($arg)+) $body)*
    } };

    (|($($parm:pat in $strategy:expr),+ $(,)?)| $body:expr) => {
        $crate::__proptest_internal!(
            $crate::test_runner::Config::default(),
            |($($parm in $strategy),+)| $body)
    };

    (move |($($parm:pat in $strategy:expr),+ $(,)?)| $body:expr) => {
        $crate::__proptest_internal!(
            $crate::test_runner::Config::default(),
            move |($($parm in $strategy),+)| $body)
    };

    (|($($arg:tt)+)| $body:expr) => {
        $crate::__proptest_internal!(
            $crate::test_runner::Config::default(),
            |($($arg)+)| $body)
    };

    (move |($($arg:tt)+)| $body:expr) => {
        $crate::__proptest_internal!(
            $crate::test_runner::Config::default(),
            move |($($arg)+)| $body)
    };

    ($config:expr, |($($parm:pat in $strategy:expr),+ $(,)?)| $body:expr) => { {
        let mut config = $crate::test_runner::contextualize_config($config.__sugar_to_owned());
        $crate::sugar::force_no_fork(&mut config);
        $crate::proptest_helper!(@_BODY config ($($parm in $strategy),+) [] $body)
    } };

    ($config:expr, move |($($parm:pat in $strategy:expr),+ $(,)?)| $body:expr) => { {
        let mut config = $crate::test_runner::contextualize_config($config.__sugar_to_owned());
        $crate::sugar::force_no_fork(&mut config);
        $crate::proptest_helper!(@_BODY config ($($parm in $strategy),+) [move] $body)
    } };

    ($config:expr, |($($arg:tt)+)| $body:expr) => { {
        let mut config = $crate::test_runner::contextualize_config($config.__sugar_to_owned());
        $crate::sugar::force_no_fork(&mut config);
        $crate::proptest_helper!(@_BODY2 config ($($arg)+) [] $body)
    } };

    ($config:expr, move |($($arg:tt)+)| $body:expr) => { {
        let mut config = $crate::test_runner::contextualize_config($config.__sugar_to_owned());
        $crate::sugar::force_no_fork(&mut config);
        $crate::proptest_helper!(@_BODY2 config ($($arg)+) [move] $body)
    } };
}

/// Rejects the test input if assumptions are not met.
///
/// Used directly within a function defined with `proptest!` or in any function
/// returning `Result<_, TestCaseError>`.
///
/// This is invoked as `prop_assume!(condition, format, args...)`. `condition`
/// is evaluated; if it is false, `Err(TestCaseError::Reject)` is returned. The
/// message includes the point of invocation and the format message. `format`
/// and `args` may be omitted to simply use the condition itself as the
/// message.
#[macro_export]
macro_rules! prop_assume {
    ($expr:expr) => {
        $crate::prop_assume!($expr, "{}", ::core::stringify!($expr))
    };

    ($expr:expr, $fmt:tt $(, $fmt_arg:expr),* $(,)?) => {{
        let __proptest_assumption = $expr;
        if !__proptest_assumption {
            extern crate alloc;
            return ::core::result::Result::Err(
                $crate::test_runner::TestCaseError::reject(
                    alloc::format!(::core::concat!("{}:{}:{}: ", $fmt),
                            ::core::file!(), ::core::line!(), ::core::column!()
                            $(, $fmt_arg)*)));
        }
    }};
}

/// Produce a strategy which picks one of the listed choices.
///
/// This is conceptually equivalent to calling `prop_union` on the first two
/// elements and then chaining `.or()` onto the rest after implicitly boxing
/// all of them. As with `Union`, values shrink across elements on the
/// assumption that earlier ones are "simpler", so they should be listed in
/// order of ascending complexity when possible.
///
/// The macro invocation has two forms. The first is to simply list the
/// strategies separated by commas; this will cause value generation to pick
/// from the strategies uniformly. The other form is to provide a weight in the
/// form of a `u32` before each strategy, separated from the strategy with
/// `=>`.
///
/// Note that the exact type returned by the macro varies depending on how many
/// inputs there are. In particular, if given exactly one option, it will
/// return it unmodified. It is not recommended to depend on the particular
/// type produced by this macro.
///
/// ## Example
///
/// ```rust,no_run
/// use proptest::prelude::*;
///
/// #[derive(Clone, Copy, Debug)]
/// enum MyEnum {
///   Big(u64),
///   Medium(u32),
///   Little(i16),
/// }
///
/// # #[allow(unused_variables)]
/// # fn main() {
/// let my_enum_strategy = prop_oneof![
///   prop::num::i16::ANY.prop_map(MyEnum::Little),
///   prop::num::u32::ANY.prop_map(MyEnum::Medium),
///   prop::num::u64::ANY.prop_map(MyEnum::Big),
/// ];
///
/// let my_weighted_strategy = prop_oneof![
///   1 => prop::num::i16::ANY.prop_map(MyEnum::Little),
///   // Chose `Medium` twice as frequently as either `Little` or `Big`; i.e.,
///   // around 50% of values will be `Medium`, and 25% for each of `Little`
///   // and `Big`.
///   2 => prop::num::u32::ANY.prop_map(MyEnum::Medium),
///   1 => prop::num::u64::ANY.prop_map(MyEnum::Big),
/// ];
/// # }
/// ```
#[macro_export]
macro_rules! prop_oneof {
    ($($item:expr),+ $(,)?) => {
        $crate::prop_oneof![
            $(1 => $item),+
        ]
    };

    ($_weight0:expr => $item0:expr $(,)?) => { $item0 };

    ($weight0:expr => $item0:expr,
     $weight1:expr => $item1:expr $(,)?) => {{
        $crate::strategy::TupleUnion::new(
            (($weight0, $crate::std_facade::Rc::new($item0)),
             ($weight1, $crate::std_facade::Rc::new($item1))))
    }};

    ($weight0:expr => $item0:expr,
     $weight1:expr => $item1:expr,
     $weight2:expr => $item2:expr $(,)?) => {{
        $crate::strategy::TupleUnion::new(
            (($weight0, $crate::std_facade::Rc::new($item0)),
             ($weight1, $crate::std_facade::Rc::new($item1)),
             ($weight2, $crate::std_facade::Rc::new($item2))))
    }};

    ($weight0:expr => $item0:expr,
     $weight1:expr => $item1:expr,
     $weight2:expr => $item2:expr,
     $weight3:expr => $item3:expr $(,)?) => {{
        $crate::strategy::TupleUnion::new(
            (($weight0, $crate::std_facade::Rc::new($item0)),
             ($weight1, $crate::std_facade::Rc::new($item1)),
             ($weight2, $crate::std_facade::Rc::new($item2)),
             ($weight3, $crate::std_facade::Rc::new($item3))))
    }};

    ($weight0:expr => $item0:expr,
     $weight1:expr => $item1:expr,
     $weight2:expr => $item2:expr,
     $weight3:expr => $item3:expr,
     $weight4:expr => $item4:expr $(,)?) => {{
        $crate::strategy::TupleUnion::new(
            (($weight0, $crate::std_facade::Rc::new($item0)),
             ($weight1, $crate::std_facade::Rc::new($item1)),
             ($weight2, $crate::std_facade::Rc::new($item2)),
             ($weight3, $crate::std_facade::Rc::new($item3)),
             ($weight4, $crate::std_facade::Rc::new($item4))))
    }};

    ($weight0:expr => $item0:expr,
     $weight1:expr => $item1:expr,
     $weight2:expr => $item2:expr,
     $weight3:expr => $item3:expr,
     $weight4:expr => $item4:expr,
     $weight5:expr => $item5:expr $(,)?) => {{
        $crate::strategy::TupleUnion::new(
            (($weight0, $crate::std_facade::Rc::new($item0)),
             ($weight1, $crate::std_facade::Rc::new($item1)),
             ($weight2, $crate::std_facade::Rc::new($item2)),
             ($weight3, $crate::std_facade::Rc::new($item3)),
             ($weight4, $crate::std_facade::Rc::new($item4)),
             ($weight5, $crate::std_facade::Rc::new($item5))))
    }};

    ($weight0:expr => $item0:expr,
     $weight1:expr => $item1:expr,
     $weight2:expr => $item2:expr,
     $weight3:expr => $item3:expr,
     $weight4:expr => $item4:expr,
     $weight5:expr => $item5:expr,
     $weight6:expr => $item6:expr $(,)?) => {{
        $crate::strategy::TupleUnion::new(
            (($weight0, $crate::std_facade::Rc::new($item0)),
             ($weight1, $crate::std_facade::Rc::new($item1)),
             ($weight2, $crate::std_facade::Rc::new($item2)),
             ($weight3, $crate::std_facade::Rc::new($item3)),
             ($weight4, $crate::std_facade::Rc::new($item4)),
             ($weight5, $crate::std_facade::Rc::new($item5)),
             ($weight6, $crate::std_facade::Rc::new($item6))))
    }};

    ($weight0:expr => $item0:expr,
     $weight1:expr => $item1:expr,
     $weight2:expr => $item2:expr,
     $weight3:expr => $item3:expr,
     $weight4:expr => $item4:expr,
     $weight5:expr => $item5:expr,
     $weight6:expr => $item6:expr,
     $weight7:expr => $item7:expr $(,)?) => {{
        $crate::strategy::TupleUnion::new(
            (($weight0, $crate::std_facade::Rc::new($item0)),
             ($weight1, $crate::std_facade::Rc::new($item1)),
             ($weight2, $crate::std_facade::Rc::new($item2)),
             ($weight3, $crate::std_facade::Rc::new($item3)),
             ($weight4, $crate::std_facade::Rc::new($item4)),
             ($weight5, $crate::std_facade::Rc::new($item5)),
             ($weight6, $crate::std_facade::Rc::new($item6)),
             ($weight7, $crate::std_facade::Rc::new($item7))))
    }};

    ($weight0:expr => $item0:expr,
     $weight1:expr => $item1:expr,
     $weight2:expr => $item2:expr,
     $weight3:expr => $item3:expr,
     $weight4:expr => $item4:expr,
     $weight5:expr => $item5:expr,
     $weight6:expr => $item6:expr,
     $weight7:expr => $item7:expr,
     $weight8:expr => $item8:expr $(,)?) => {{
        $crate::strategy::TupleUnion::new(
            (($weight0, $crate::std_facade::Rc::new($item0)),
             ($weight1, $crate::std_facade::Rc::new($item1)),
             ($weight2, $crate::std_facade::Rc::new($item2)),
             ($weight3, $crate::std_facade::Rc::new($item3)),
             ($weight4, $crate::std_facade::Rc::new($item4)),
             ($weight5, $crate::std_facade::Rc::new($item5)),
             ($weight6, $crate::std_facade::Rc::new($item6)),
             ($weight7, $crate::std_facade::Rc::new($item7)),
             ($weight8, $crate::std_facade::Rc::new($item8))))
    }};

    ($weight0:expr => $item0:expr,
     $weight1:expr => $item1:expr,
     $weight2:expr => $item2:expr,
     $weight3:expr => $item3:expr,
     $weight4:expr => $item4:expr,
     $weight5:expr => $item5:expr,
     $weight6:expr => $item6:expr,
     $weight7:expr => $item7:expr,
     $weight8:expr => $item8:expr,
     $weight9:expr => $item9:expr $(,)?) => {{
        $crate::strategy::TupleUnion::new(
            (($weight0, $crate::std_facade::Rc::new($item0)),
             ($weight1, $crate::std_facade::Rc::new($item1)),
             ($weight2, $crate::std_facade::Rc::new($item2)),
             ($weight3, $crate::std_facade::Rc::new($item3)),
             ($weight4, $crate::std_facade::Rc::new($item4)),
             ($weight5, $crate::std_facade::Rc::new($item5)),
             ($weight6, $crate::std_facade::Rc::new($item6)),
             ($weight7, $crate::std_facade::Rc::new($item7)),
             ($weight8, $crate::std_facade::Rc::new($item8)),
             ($weight9, $crate::std_facade::Rc::new($item9))))
    }};

    ($($weight:expr => $item:expr),+ $(,)?) => {
        $crate::strategy::Union::new_weighted($crate::std_facade::vec![
            $(($weight, $crate::strategy::Strategy::boxed($item))),+
        ])
    };
}

/// Convenience to define functions which produce new strategies.
///
/// The macro has two general forms. In the first, you define a function with
/// two argument lists. The first argument list uses the usual syntax and
/// becomes exactly the argument list of the defined function. The second
/// argument list uses the `in strategy` syntax as with `proptest!`, and is
/// used to generate the other inputs for the function. The second argument
/// list has access to all arguments in the first. The return type indicates
/// the type of value being generated; the final return type of the function is
/// `impl Strategy<Value = $type>`.
///
/// ```rust,no_run
/// # #![allow(dead_code)]
/// use proptest::prelude::*;
///
/// #[derive(Clone, Debug)]
/// struct MyStruct {
///   integer: u32,
///   string:  String,
/// }
///
/// prop_compose! {
///   fn my_struct_strategy(max_integer: u32)
///                        (integer in 0..max_integer, string in ".*")
///                        -> MyStruct {
///     MyStruct { integer, string }
///   }
/// }
/// #
/// # fn main() { }
/// ```
///
/// This form is simply sugar around making a tuple and then calling `prop_map`
/// on it. You can also use `arg: type` as in `proptest! { .. }`:
///
/// ```rust,no_run
/// # #![allow(dead_code)]
/// # use proptest::prelude::*;
/// #
/// # #[derive(Clone, Debug)]
/// # struct MyStruct {
/// #  integer: u32,
/// #  string: String,
/// # }
///
/// prop_compose! {
///   fn my_struct_strategy(max_integer: u32)
///                        (integer in 0..max_integer, string: String)
///                        -> MyStruct {
///     MyStruct { integer, string }
///   }
/// }
/// #
/// # fn main() { }
/// ```
///
/// The second form is mostly the same, except that it takes _three_ argument
/// lists. The third argument list can see all values in both prior, which
/// permits producing strategies based on other strategies.
///
/// ```rust,no_run
/// # #![allow(dead_code)]
/// use proptest::prelude::*;
///
/// prop_compose! {
///   fn nearby_numbers()(centre in -1000..1000)
///                    (a in centre-10..centre+10,
///                     b in centre-10..centre+10)
///                    -> (i32, i32) {
///     (a, b)
///   }
/// }
/// #
/// # fn main() { }
/// ```
///
/// However, the body of the function does _not_ have access to the second
/// argument list. If the body needs access to those values, they must be
/// passed through explicitly.
///
/// ```rust,no_run
/// # #![allow(dead_code)]
/// use proptest::prelude::*;
///
/// prop_compose! {
///   fn vec_and_index
///     (max_length: usize)
///     (vec in prop::collection::vec(1..10, 1..max_length))
///     (index in 0..vec.len(), vec in Just(vec))
///     -> (Vec<i32>, usize)
///   {
///     (vec, index)
///   }
/// }
/// # fn main() { }
/// ```
///
/// The second form is sugar around making a strategy tuple, calling
/// `prop_flat_map()`, then `prop_map()`.
///
/// Visibility modifiers are supported directly before the `fn` token. Other
/// bracketed function modifiers are rejected; use [`prop_compose_ffi!`] when a
/// generated strategy should call a C-ABI mapper.
///
/// ```rust,no_run
/// # #![allow(dead_code)]
/// use proptest::prelude::*;
///
/// prop_compose! {
///   pub(crate) fn pointer_sized()(v in prop::num::usize::ANY) -> usize {
///     v
///   }
/// }
/// # fn main() { }
/// ```
///
/// ## Comparison with Hypothesis' `@composite`
///
/// `prop_compose!` makes it easy to do a lot of things you can do with
/// [Hypothesis' `@composite`](https://hypothesis.readthedocs.io/en/latest/data.html#composite-strategies),
/// but not everything.
///
/// - You can't filter via this macro. For filtering, you need to make the strategy the "normal" way
///   and use `prop_filter()`.
///
/// - More than two layers of strategies or arbitrary logic between the two layers. If you need
///   either of these, you can achieve them by calling `prop_flat_map()` by hand.
#[macro_export]
macro_rules! prop_compose {
    ($(#[$meta:meta])*
     $vis:vis
     [$($modifier:tt)*] fn $name:ident $($tail:tt)*) =>
    {
        ::core::compile_error!(
            "prop_compose! no longer supports bracketed function modifiers; use prop_compose_ffi! for C-ABI mapper functions"
        );
    };

    ($(#[$meta:meta])*
     $vis:vis
     fn $name:ident $params:tt
     ($($var:pat in $strategy:expr),+ $(,)?)
       -> $return_type:ty $body:block) =>
    {
        #[must_use = "strategies do nothing unless used"]
        $(#[$meta])*
        $vis
        fn $name $params
                 -> impl $crate::strategy::Strategy<Value = $return_type> {
            let strat = $crate::proptest_helper!(@_WRAP ($($strategy)+));
            $crate::strategy::Strategy::prop_map(strat,
                move |$crate::proptest_helper!(@_WRAPPAT ($($var),+))| $body)
        }
    };

    ($(#[$meta:meta])*
     $vis:vis
     fn $name:ident $params:tt
     ($($var:pat in $strategy:expr),+ $(,)?)
     ($($var2:pat in $strategy2:expr),+ $(,)?)
       -> $return_type:ty $body:block) =>
    {
        #[must_use = "strategies do nothing unless used"]
        $(#[$meta])*
        $vis
        fn $name $params
                 -> impl $crate::strategy::Strategy<Value = $return_type> {
            let strat = $crate::proptest_helper!(@_WRAP ($($strategy)+));
            let strat = $crate::strategy::Strategy::prop_flat_map(
                strat,
                move |$crate::proptest_helper!(@_WRAPPAT ($($var),+))|
                $crate::proptest_helper!(@_WRAP ($($strategy2)+)));
            $crate::strategy::Strategy::prop_map(strat,
                move |$crate::proptest_helper!(@_WRAPPAT ($($var2),+))| $body)
        }
    };

    ($(#[$meta:meta])*
     $vis:vis
     fn $name:ident $params:tt
     ($($arg:tt)+)
       -> $return_type:ty $body:block) =>
    {
        #[must_use = "strategies do nothing unless used"]
        $(#[$meta])*
        $vis
        fn $name $params
                 -> impl $crate::strategy::Strategy<Value = $return_type> {
            let strat = $crate::proptest_helper!(@_EXT _STRAT ($($arg)+));
            $crate::strategy::Strategy::prop_map(strat,
                move |$crate::proptest_helper!(@_EXT _PAT ($($arg)+))| $body)
        }
    };

    ($(#[$meta:meta])*
     $vis:vis
     fn $name:ident $params:tt
     ($($arg:tt)+)
     ($($arg2:tt)+)
       -> $return_type:ty $body:block) =>
    {
        #[must_use = "strategies do nothing unless used"]
        $(#[$meta])*
        $vis
        fn $name $params
                 -> impl $crate::strategy::Strategy<Value = $return_type> {
            let strat = $crate::proptest_helper!(@_EXT _STRAT ($($arg)+));
            let strat = $crate::strategy::Strategy::prop_flat_map(
                strat,
                move |$crate::proptest_helper!(@_EXT _PAT ($($arg)+))|
                $crate::proptest_helper!(@_EXT _STRAT ($($arg2)+)));
            $crate::strategy::Strategy::prop_map(strat,
                move |$crate::proptest_helper!(@_EXT _PAT ($($arg2)+))| $body)
        }
    };
}

/// Define a Rust strategy-builder function that maps generated values through
/// a user-named `extern "C"` function.
///
/// The generated strategy-builder remains a normal Rust function returning
/// `impl Strategy<Value = T>`. The mapper is a local `extern "C"` item inside
/// that builder; it is not exported as a C symbol, and no raw strategy handle
/// crosses an FFI boundary.
///
/// ```rust,no_run
/// use proptest::prelude::*;
///
/// prop_compose_ffi! {
///   fn offset_sample(offset: i32)(sample in 0_i32..10)
///   with extern "C" fn add_offset(sample: i32, offset: i32) -> i32 {
///     sample + offset
///   }
///   call add_offset(sample, offset);
/// }
/// # fn main() { let _ = offset_sample(4); }
/// ```
///
/// Two-stage generation is also supported. The second strategy list can refer
/// to values generated by the first list.
///
/// ```rust,no_run
/// use proptest::prelude::*;
///
/// prop_compose_ffi! {
///   fn span_from(base: i32)
///   (upper in base + 1..base + 8)
///   (lower in base..upper, upper in Just(upper))
///   with extern "C" fn span(lower: i32, upper: i32) -> i32 {
///     upper - lower
///   }
///   call span(lower, upper);
/// }
/// # fn main() { let _ = span_from(3); }
/// ```
#[macro_export]
macro_rules! prop_compose_ffi {
    ($(#[$meta:meta])*
     $vis:vis fn $name:ident $params:tt
     ($($var:pat in $strategy:expr),+ $(,)?)
     with extern "C" fn $mapper:ident(
         $($mapper_arg:ident : $mapper_ty:ty),* $(,)?
     ) -> $return_type:ty $mapper_body:block
     call $mapper_call:expr;) =>
    {
        #[must_use = "strategies do nothing unless used"]
        $(#[$meta])*
        $vis fn $name $params
                 -> impl $crate::strategy::Strategy<Value = $return_type> {
            #[allow(
                clippy::single_call_fn,
                reason = "prop_compose_ffi preserves the user-named C-ABI mapper as a local function item"
            )]
            extern "C" fn $mapper(
                $($mapper_arg : $mapper_ty),*
            ) -> $return_type $mapper_body

            let strat = $crate::proptest_helper!(@_WRAP ($($strategy)+));
            $crate::strategy::Strategy::prop_map(strat,
                move |$crate::proptest_helper!(@_WRAPPAT ($($var),+))|
                    $mapper_call)
        }
    };

    ($(#[$meta:meta])*
     $vis:vis fn $name:ident $params:tt
     ($($var:pat in $strategy:expr),+ $(,)?)
     ($($var2:pat in $strategy2:expr),+ $(,)?)
     with extern "C" fn $mapper:ident(
         $($mapper_arg:ident : $mapper_ty:ty),* $(,)?
     ) -> $return_type:ty $mapper_body:block
     call $mapper_call:expr;) =>
    {
        #[must_use = "strategies do nothing unless used"]
        $(#[$meta])*
        $vis fn $name $params
                 -> impl $crate::strategy::Strategy<Value = $return_type> {
            #[allow(
                clippy::single_call_fn,
                reason = "prop_compose_ffi preserves the user-named C-ABI mapper as a local function item"
            )]
            extern "C" fn $mapper(
                $($mapper_arg : $mapper_ty),*
            ) -> $return_type $mapper_body

            let strat = $crate::proptest_helper!(@_WRAP ($($strategy)+));
            let strat = $crate::strategy::Strategy::prop_flat_map(
                strat,
                move |$crate::proptest_helper!(@_WRAPPAT ($($var),+))|
                $crate::proptest_helper!(@_WRAP ($($strategy2)+)));
            $crate::strategy::Strategy::prop_map(strat,
                move |$crate::proptest_helper!(@_WRAPPAT ($($var2),+))|
                    $mapper_call)
        }
    };

    ($(#[$meta:meta])*
     $vis:vis fn $name:ident $params:tt
     ($($arg:tt)+)
     with extern "C" fn $mapper:ident(
         $($mapper_arg:ident : $mapper_ty:ty),* $(,)?
     ) -> $return_type:ty $mapper_body:block
     call $mapper_call:expr;) =>
    {
        #[must_use = "strategies do nothing unless used"]
        $(#[$meta])*
        $vis fn $name $params
                 -> impl $crate::strategy::Strategy<Value = $return_type> {
            #[allow(
                clippy::single_call_fn,
                reason = "prop_compose_ffi preserves the user-named C-ABI mapper as a local function item"
            )]
            extern "C" fn $mapper(
                $($mapper_arg : $mapper_ty),*
            ) -> $return_type $mapper_body

            let strat = $crate::proptest_helper!(@_EXT _STRAT ($($arg)+));
            $crate::strategy::Strategy::prop_map(strat,
                move |$crate::proptest_helper!(@_EXT _PAT ($($arg)+))|
                    $mapper_call)
        }
    };

    ($(#[$meta:meta])*
     $vis:vis fn $name:ident $params:tt
     ($($arg:tt)+)
     ($($arg2:tt)+)
     with extern "C" fn $mapper:ident(
         $($mapper_arg:ident : $mapper_ty:ty),* $(,)?
     ) -> $return_type:ty $mapper_body:block
     call $mapper_call:expr;) =>
    {
        #[must_use = "strategies do nothing unless used"]
        $(#[$meta])*
        $vis fn $name $params
                 -> impl $crate::strategy::Strategy<Value = $return_type> {
            #[allow(
                clippy::single_call_fn,
                reason = "prop_compose_ffi preserves the user-named C-ABI mapper as a local function item"
            )]
            extern "C" fn $mapper(
                $($mapper_arg : $mapper_ty),*
            ) -> $return_type $mapper_body

            let strat = $crate::proptest_helper!(@_EXT _STRAT ($($arg)+));
            let strat = $crate::strategy::Strategy::prop_flat_map(
                strat,
                move |$crate::proptest_helper!(@_EXT _PAT ($($arg)+))|
                $crate::proptest_helper!(@_EXT _STRAT ($($arg2)+)));
            $crate::strategy::Strategy::prop_map(strat,
                move |$crate::proptest_helper!(@_EXT _PAT ($($arg2)+))|
                    $mapper_call)
        }
    };
}

/// Similar to `assert!` from std, but returns a test failure instead of
/// panicking if the condition fails.
///
/// This can be used in any function that returns a `Result<_, TestCaseError>`,
/// including the top-level function inside `proptest!`.
///
/// Both panicking via `assert!` and returning a test case failure have the
/// same effect as far as proptest is concerned; however, the Rust runtime
/// implicitly prints every panic to stderr by default (including a backtrace
/// if enabled), which can make test failures unnecessarily noisy. By using
/// `prop_assert!` instead, the only output on a failing test case is the final
/// panic including the minimal test case.
///
/// ## Example
///
/// ```
/// use proptest::prelude::*;
///
/// proptest! {
///   # /*
///   #[test]
///   # */
///   fn triangle_inequality(a in 0.0_f64..10.0, b in 0.0_f64..10.0) {
///     // Called with just a condition will print the condition on failure
///     prop_assert!((a*a + b*b).sqrt() <= a + b);
///     // You can also provide a custom failure message
///     prop_assert!((a*a + b*b).sqrt() <= a + b,
///                  "Triangle inequality didn't hold for ({}, {})", a, b);
///     // If calling another function that can return failure, don't forget
///     // the `?` to propagate the failure.
///     assert_from_other_function(a, b)?;
///   }
/// }
///
/// // The macro can be used from another function provided it has a compatible
/// // return type.
/// fn assert_from_other_function(a: f64, b: f64) -> Result<(), TestCaseError> {
///   prop_assert!((a * a + b * b).sqrt() <= a + b);
///   Ok(())
/// }
/// #
/// # fn main() { triangle_inequality(); }
/// ```
#[macro_export]
macro_rules! prop_assert {
    ($cond:expr $(,) ?) => {
        $crate::prop_assert!($cond, ::core::concat!("assertion failed: ", ::core::stringify!($cond)))
    };

    ($cond:expr, $($fmt:tt)*) => {{
        let __proptest_assertion = $cond;
        if !__proptest_assertion {
            extern crate alloc;
            let message = alloc::format!($($fmt)*);
            let message = alloc::format!("{} at {}:{}", message, ::core::file!(), ::core::line!());
            return ::core::result::Result::Err(
                $crate::test_runner::TestCaseError::fail(message));
        }
    }};
}

/// Similar to `assert_eq!` from std, but returns a test failure instead of
/// panicking if the condition fails.
///
/// See `prop_assert!` for a more in-depth discussion.
///
/// ## Example
///
/// ```
/// use proptest::prelude::*;
///
/// proptest! {
///   # /*
///   #[test]
///   # */
///   fn concat_string_length(ref a in ".*", ref b in ".*") {
///     let cat = format!("{}{}", a, b);
///     // Use with default message
///     prop_assert_eq!(a.len() + b.len(), cat.len());
///     // Can also provide custom message (added after the normal
///     // assertion message)
///     prop_assert_eq!(a.len() + b.len(), cat.len(),
///                     "a = {:?}, b = {:?}", a, b);
///   }
/// }
/// #
/// # fn main() { concat_string_length(); }
/// ```
#[macro_export]
macro_rules! prop_assert_eq {
    ($left:expr, $right:expr $(,) ?) => {{
        let left = $left;
        let right = $right;
        $crate::prop_assert!(
            left == right,
            "assertion failed: `(left == right)` \
             \n  left: `{:?}`,\n right: `{:?}`",
            left, right);
    }};

    ($left:expr, $right:expr, $fmt:tt $($args:tt)*) => {{
        let left = $left;
        let right = $right;
        $crate::prop_assert!(
            left == right,
            concat!(
                "assertion failed: `(left == right)` \
                 \n  left: `{:?}`, \n right: `{:?}`: ", $fmt),
            left, right $($args)*);
    }};
}

/// Similar to `assert_ne!` from std, but returns a test failure instead of
/// panicking if the condition fails.
///
/// See `prop_assert!` for a more in-depth discussion.
///
/// ## Example
///
/// ```
/// use proptest::prelude::*;
///
/// proptest! {
///   # /*
///   #[test]
///   # */
///   fn test_addition(a in 0_i32..100_i32, b in 1_i32..100_i32) {
///     // Use with default message
///     prop_assert_ne!(a, a + b);
///     // Can also provide custom message added after the common message
///     prop_assert_ne!(a, a + b, "a = {}, b = {}", a, b);
///   }
/// }
/// #
/// # fn main() { test_addition(); }
/// ```
#[macro_export]
macro_rules! prop_assert_ne {
    ($left:expr, $right:expr $(,) ?) => {{
        let left = $left;
        let right = $right;
        $crate::prop_assert!(
            left != right,
            "assertion failed: `(left != right)`\
             \n  left: `{:?}`,\n right: `{:?}`",
            left, right);
    }};

    ($left:expr, $right:expr, $fmt:tt $($args:tt)*) => {{
        let left = $left;
        let right = $right;
        $crate::prop_assert!(left != right, concat!(
                "assertion failed: `(left != right)`\
                 \n  left: `{:?}`,\n right: `{:?}`: ", $fmt),
            left, right $($args)*);
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! proptest_helper {
    (@_WRAP ($a:tt)) => { $a };
    (@_WRAP ($a0:tt $a1:tt)) => { ($a0, $a1) };
    (@_WRAP ($a0:tt $a1:tt $a2:tt)) => { ($a0, $a1, $a2) };
    (@_WRAP ($a0:tt $a1:tt $a2:tt $a3:tt)) => { ($a0, $a1, $a2, $a3) };
    (@_WRAP ($a0:tt $a1:tt $a2:tt $a3:tt $a4:tt)) => {
        ($a0, $a1, $a2, $a3, $a4)
    };
    (@_WRAP ($a0:tt $a1:tt $a2:tt $a3:tt $a4:tt $a5:tt)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5)
    };
    (@_WRAP ($a0:tt $a1:tt $a2:tt $a3:tt $a4:tt $a5:tt $a6:tt)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5, $a6)
    };
    (@_WRAP ($a0:tt $a1:tt $a2:tt $a3:tt
             $a4:tt $a5:tt $a6:tt $a7:tt)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5, $a6, $a7)
    };
    (@_WRAP ($a0:tt $a1:tt $a2:tt $a3:tt $a4:tt
             $a5:tt $a6:tt $a7:tt $a8:tt)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5, $a6, $a7, $a8)
    };
    (@_WRAP ($a0:tt $a1:tt $a2:tt $a3:tt $a4:tt
             $a5:tt $a6:tt $a7:tt $a8:tt $a9:tt)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5, $a6, $a7, $a8, $a9)
    };
    (@_WRAP ($a:tt $($rest:tt)*)) => {
        ($a, $crate::proptest_helper!(@_WRAP ($($rest)*)))
    };
    (@_WRAPPAT ($item:pat)) => { $item };
    (@_WRAPPAT ($a0:pat, $a1:pat)) => { ($a0, $a1) };
    (@_WRAPPAT ($a0:pat, $a1:pat, $a2:pat)) => { ($a0, $a1, $a2) };
    (@_WRAPPAT ($a0:pat, $a1:pat, $a2:pat, $a3:pat)) => {
        ($a0, $a1, $a2, $a3)
    };
    (@_WRAPPAT ($a0:pat, $a1:pat, $a2:pat, $a3:pat, $a4:pat)) => {
        ($a0, $a1, $a2, $a3, $a4)
    };
    (@_WRAPPAT ($a0:pat, $a1:pat, $a2:pat, $a3:pat, $a4:pat, $a5:pat)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5)
    };
    (@_WRAPPAT ($a0:pat, $a1:pat, $a2:pat, $a3:pat,
                $a4:pat, $a5:pat, $a6:pat)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5, $a6)
    };
    (@_WRAPPAT ($a0:pat, $a1:pat, $a2:pat, $a3:pat,
                $a4:pat, $a5:pat, $a6:pat, $a7:pat)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5, $a6, $a7)
    };
    (@_WRAPPAT ($a0:pat, $a1:pat, $a2:pat, $a3:pat, $a4:pat,
                $a5:pat, $a6:pat, $a7:pat, $a8:pat)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5, $a6, $a7, $a8)
    };
    (@_WRAPPAT ($a0:pat, $a1:pat, $a2:pat, $a3:pat, $a4:pat,
                $a5:pat, $a6:pat, $a7:pat, $a8:pat, $a9:pat)) => {
        ($a0, $a1, $a2, $a3, $a4, $a5, $a6, $a7, $a8, $a9)
    };
    (@_WRAPPAT ($a:pat, $($rest:pat),*)) => {
        ($a, $crate::proptest_helper!(@_WRAPPAT ($($rest),*)))
    };
    (@_WRAPSTR ($item:pat)) => { ::core::stringify!($item) };
    (@_WRAPSTR ($a0:pat, $a1:pat)) => { (::core::stringify!($a0), ::core::stringify!($a1)) };
    (@_WRAPSTR ($a0:pat, $a1:pat, $a2:pat)) => {
        (::core::stringify!($a0), ::core::stringify!($a1), ::core::stringify!($a2))
    };
    (@_WRAPSTR ($a0:pat, $a1:pat, $a2:pat, $a3:pat)) => {
        (::core::stringify!($a0), ::core::stringify!($a1), ::core::stringify!($a2), ::core::stringify!($a3))
    };
    (@_WRAPSTR ($a0:pat, $a1:pat, $a2:pat, $a3:pat, $a4:pat)) => {
        (::core::stringify!($a0), ::core::stringify!($a1), ::core::stringify!($a2),
         ::core::stringify!($a3), ::core::stringify!($a4))
    };
    (@_WRAPSTR ($a0:pat, $a1:pat, $a2:pat, $a3:pat, $a4:pat, $a5:pat)) => {
        (::core::stringify!($a0), ::core::stringify!($a1), ::core::stringify!($a2), ::core::stringify!($a3),
         ::core::stringify!($a4), ::core::stringify!($a5))
    };
    (@_WRAPSTR ($a0:pat, $a1:pat, $a2:pat, $a3:pat,
                $a4:pat, $a5:pat, $a6:pat)) => {
        (::core::stringify!($a0), ::core::stringify!($a1), ::core::stringify!($a2), ::core::stringify!($a3),
         ::core::stringify!($a4), ::core::stringify!($a5), ::core::stringify!($a6))
    };
    (@_WRAPSTR ($a0:pat, $a1:pat, $a2:pat, $a3:pat,
                $a4:pat, $a5:pat, $a6:pat, $a7:pat)) => {
        (::core::stringify!($a0), ::core::stringify!($a1), ::core::stringify!($a2), ::core::stringify!($a3),
         ::core::stringify!($a4), ::core::stringify!($a5), ::core::stringify!($a6), ::core::stringify!($a7))
    };
    (@_WRAPSTR ($a0:pat, $a1:pat, $a2:pat, $a3:pat, $a4:pat,
                $a5:pat, $a6:pat, $a7:pat, $a8:pat)) => {
        (::core::stringify!($a0), ::core::stringify!($a1), ::core::stringify!($a2), ::core::stringify!($a3),
         ::core::stringify!($a4), ::core::stringify!($a5), ::core::stringify!($a6), ::core::stringify!($a7),
         ::core::stringify!($a8))
    };
    (@_WRAPSTR ($a0:pat, $a1:pat, $a2:pat, $a3:pat, $a4:pat,
                $a5:pat, $a6:pat, $a7:pat, $a8:pat, $a9:pat)) => {
        (::core::stringify!($a0), ::core::stringify!($a1), ::core::stringify!($a2), ::core::stringify!($a3),
         ::core::stringify!($a4), ::core::stringify!($a5), ::core::stringify!($a6), ::core::stringify!($a7),
         ::core::stringify!($a8), ::core::stringify!($a9))
    };
    (@_WRAPSTR ($a:pat, $($rest:pat),*)) => {
        (::core::stringify!($a), $crate::proptest_helper!(@_WRAPSTR ($($rest),*)))
    };
    // build a property testing block that when executed, executes the full property test.
    (@_BODY $config:ident ($($parm:pat in $strategy:expr),+) [$($mod:tt)*] $body:expr) => {{
        $config.source_file = Some(file!());
        let mut runner = $crate::test_runner::TestRunner::new($config);
        let names = $crate::proptest_helper!(@_WRAPSTR ($($parm),+));
        runner.run(
            &$crate::strategy::Strategy::prop_map(
                $crate::proptest_helper!(@_WRAP ($($strategy)+)),
                |values| $crate::sugar::NamedArguments(names, values)),
            $($mod)* |$crate::sugar::NamedArguments(
                _, $crate::proptest_helper!(@_WRAPPAT ($($parm),+)))|
            {
                let (): () = $body;
                ::core::result::Result::Ok(())
            })
            .map_err(|error| {
                let boxed: $crate::std_facade::Box<dyn ::core::fmt::Debug> =
                    $crate::std_facade::Box::new(error);
                boxed
            })
    }};
    // build a property testing block that when executed, executes the full property test.
    (@_BODY2 $config:ident ($($arg:tt)+) [$($mod:tt)*] $body:expr) => {{
        $config.source_file = Some(::core::file!());
        let mut runner = $crate::test_runner::TestRunner::new($config);
        let names = $crate::proptest_helper!(@_EXT _STR ($($arg)+));
        runner.run(
            &$crate::strategy::Strategy::prop_map(
                $crate::proptest_helper!(@_EXT _STRAT ($($arg)+)),
                |values| $crate::sugar::NamedArguments(names, values)),
            $($mod)* |$crate::sugar::NamedArguments(
                _, $crate::proptest_helper!(@_EXT _PAT ($($arg)+)))|
            {
                let (): () = $body;
                ::core::result::Result::Ok(())
            })
            .map_err(|error| {
                let boxed: $crate::std_facade::Box<dyn ::core::fmt::Debug> =
                    $crate::std_facade::Box::new(error);
                boxed
            })
    }};

    // The logic below helps support `pat: type` in the proptest! macro.

    // These matchers define the actual logic:
    (@_STRAT [$s:ty] [$p:pat]) => { $crate::arbitrary::any::<$s>()  };
    (@_PAT [$s:ty] [$p:pat]) => { $p };
    (@_STR [$s:ty] [$p:pat]) => { ::core::stringify!($p) };
    (@_STRAT in [$s:expr] [$p:pat]) => { $s };
    (@_PAT in [$s:expr] [$p:pat]) => { $p };
    (@_STR in [$s:expr] [$p:pat]) => { ::core::stringify!($p) };

    // These matchers rewrite into the above extractors.
    // We have to do this because `:` can't FOLLOW(pat).
    // Note that this is not the full `pat` grammar...
    // See https://docs.rs/syn/0.14.2/syn/enum.Pat.html for that.
    (@_EXT $cmd:ident ($p:pat in $s:expr $(,)?)) => {
        $crate::proptest_helper!(@$cmd in [$s] [$p])
    };
    (@_EXT $cmd:ident (($p:pat) : $s:ty $(,)?)) => {
        // Users can wrap in parens as a last resort.
        $crate::proptest_helper!(@$cmd [$s] [$p])
    };
    (@_EXT $cmd:ident (_ : $s:ty $(,)?)) => {
        $crate::proptest_helper!(@$cmd [$s] [_])
    };
    (@_EXT $cmd:ident (ref mut $p:ident : $s:ty $(,)?)) => {
        $crate::proptest_helper!(@$cmd [$s] [ref mut $p])
    };
    (@_EXT $cmd:ident (ref $p:ident : $s:ty $(,)?)) => {
        $crate::proptest_helper!(@$cmd [$s] [ref $p])
    };
    (@_EXT $cmd:ident (mut $p:ident : $s:ty $(,)?)) => {
        $crate::proptest_helper!(@$cmd [$s] [mut $p])
    };
    (@_EXT $cmd:ident ($p:ident : $s:ty $(,)?)) => {
        $crate::proptest_helper!(@$cmd [$s] [$p])
    };
    (@_EXT $cmd:ident ([$($p:tt)*] : $s:ty $(,)?)) => {
        $crate::proptest_helper!(@$cmd [$s] [[$($p)*]])
    };

    // Rewrite, Inductive case:
    (@_EXT $cmd:ident ($p:pat in $s:expr, $($r:tt)*)) => {
        ($crate::proptest_helper!(@$cmd in [$s] [$p]), $crate::proptest_helper!(@_EXT $cmd ($($r)*)))
    };
    (@_EXT $cmd:ident (($p:pat) : $s:ty, $($r:tt)*)) => {
        ($crate::proptest_helper!(@$cmd [$s] [$p]), $crate::proptest_helper!(@_EXT $cmd ($($r)*)))
    };
    (@_EXT $cmd:ident (_ : $s:ty, $($r:tt)*)) => {
        ($crate::proptest_helper!(@$cmd [$s] [_]), $crate::proptest_helper!(@_EXT $cmd ($($r)*)))
    };
    (@_EXT $cmd:ident (ref mut $p:ident : $s:ty, $($r:tt)*)) => {
        ($crate::proptest_helper!(@$cmd [$s] [ref mut $p]), $crate::proptest_helper!(@_EXT $cmd ($($r)*)))
    };
    (@_EXT $cmd:ident (ref $p:ident : $s:ty, $($r:tt)*)) => {
        ($crate::proptest_helper!(@$cmd [$s] [ref $p]), $crate::proptest_helper!(@_EXT $cmd ($($r)*)))
    };
    (@_EXT $cmd:ident (mut $p:ident : $s:ty, $($r:tt)*)) => {
        ($crate::proptest_helper!(@$cmd [$s] [mut $p]), $crate::proptest_helper!(@_EXT $cmd ($($r)*)))
    };
    (@_EXT $cmd:ident ($p:ident : $s:ty, $($r:tt)*)) => {
        ($crate::proptest_helper!(@$cmd [$s] [$p]), $crate::proptest_helper!(@_EXT $cmd ($($r)*)))
    };
    (@_EXT $cmd:ident ([$($p:tt)*] : $s:ty, $($r:tt)*)) => {
        ($crate::proptest_helper!(@$cmd [$s] [[$($p)*]]), $crate::proptest_helper!(@_EXT $cmd ($($r)*)))
    };
}

#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct NamedArguments<N, V>(#[doc(hidden)] pub N, #[doc(hidden)] pub V);

impl<V: fmt::Debug> fmt::Debug for NamedArguments<&'static str, V> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{} = ", self.0)?;
    self.1.fmt(f)
  }
}

macro_rules! named_arguments_tuple {
    ($first_ix:tt $first_argn:ident $first_argv:ident
     $($ix:tt $argn:ident $argv:ident)*) => {
        impl<
            'a,
            $first_argn: Copy,
            $($argn: Copy,)*
            $first_argv,
            $($argv,)*
        > fmt::Debug
        for NamedArguments<
            ($first_argn, $($argn,)*),
            &'a ($first_argv, $($argv,)*)
        >
        where
            NamedArguments<$first_argn, &'a $first_argv>: fmt::Debug,
            $(NamedArguments<$argn, &'a $argv>: fmt::Debug,)*
            $first_argv: 'a,
            $($argv: 'a,)*
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Debug::fmt(
                    &NamedArguments(
                        (self.0).$first_ix,
                        &(self.1).$first_ix,
                    ),
                    f,
                )?;
                $(
                    write!(f, ", ")?;
                    fmt::Debug::fmt(
                        &NamedArguments((self.0).$ix, &(self.1).$ix), f)?;
                )*
                Ok(())
            }
        }

        impl<
            $first_argn: Copy,
            $($argn: Copy,)*
            $first_argv,
            $($argv,)*
        > fmt::Debug
        for NamedArguments<
            ($first_argn, $($argn,)*),
            ($first_argv, $($argv,)*)
        >
        where
            for<'a> NamedArguments<$first_argn, &'a $first_argv>: fmt::Debug,
            $(for<'a> NamedArguments<$argn, &'a $argv>: fmt::Debug,)*
        {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Debug::fmt(
                    &NamedArguments(
                        (self.0).$first_ix,
                        &(self.1).$first_ix,
                    ),
                    f,
                )?;
                $(
                    write!(f, ", ")?;
                    fmt::Debug::fmt(
                        &NamedArguments((self.0).$ix, &(self.1).$ix), f)?;
                )*
                Ok(())
            }
        }
    }
}

named_arguments_tuple!(0 AN AV);
named_arguments_tuple!(0 AN AV 1 BN BV);
named_arguments_tuple!(0 AN AV 1 BN BV 2 CN CV);
named_arguments_tuple!(0 AN AV 1 BN BV 2 CN CV 3 DN DV);
named_arguments_tuple!(0 AN AV 1 BN BV 2 CN CV 3 DN DV 4 EN EV);
named_arguments_tuple!(0 AN AV 1 BN BV 2 CN CV 3 DN DV 4 EN EV
                       5 FN FV);
named_arguments_tuple!(0 AN AV 1 BN BV 2 CN CV 3 DN DV 4 EN EV
                       5 FN FV 6 GN GV);
named_arguments_tuple!(0 AN AV 1 BN BV 2 CN CV 3 DN DV 4 EN EV
                       5 FN FV 6 GN GV 7 HN HV);
named_arguments_tuple!(0 AN AV 1 BN BV 2 CN CV 3 DN DV 4 EN EV
                       5 FN FV 6 GN GV 7 HN HV 8 IN IV);
named_arguments_tuple!(0 AN AV 1 BN BV 2 CN CV 3 DN DV 4 EN EV
                       5 FN FV 6 GN GV 7 HN HV 8 IN IV 9 JN JV);

/// Disable fork isolation for closure-style tests when `fork` is compiled in.
#[cfg(all(feature = "std", feature = "fork"))]
#[allow(
  clippy::single_call_fn,
  reason = "name the fork-feature field update that closure-style tests must disable"
)]
const fn disable_closure_fork(config: &mut Config) {
  config.fork = false;
}

/// Preserve the same call path when `fork` is not compiled in.
#[cfg(all(feature = "std", not(feature = "fork")))]
#[allow(
  clippy::single_call_fn,
  reason = "keep the closure fork-disabling call feature-neutral when fork is absent"
)]
const fn disable_closure_fork(_: &mut Config) {}

/// Disable case timeout for closure-style tests when `timeout` is compiled in.
#[cfg(all(feature = "std", feature = "timeout"))]
#[allow(
  clippy::single_call_fn,
  reason = "name the timeout-feature field update that closure-style tests must disable"
)]
const fn disable_closure_timeout(config: &mut Config) {
  config.timeout = 0;
}

/// Preserve the same call path when `timeout` is not compiled in.
#[cfg(all(feature = "std", not(feature = "timeout")))]
#[allow(
  clippy::single_call_fn,
  reason = "keep the closure timeout-disabling call feature-neutral when timeout is absent"
)]
const fn disable_closure_timeout(_: &mut Config) {}

#[cfg(feature = "std")]
#[doc(hidden)]
pub fn force_no_fork(config: &mut Config) {
  if config.fork() {
    emit_closure_fork_unsupported();

    disable_closure_fork(config);
    disable_closure_timeout(config);
  }
}

#[cfg(not(feature = "std"))]
pub fn force_no_fork(_: &mut crate::test_runner::Config) {}

#[cfg(test)]
mod test {
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_some;

  use crate::std_facade::ToOwned as _;
  use crate::strategy::Just;
  use crate::strategy::Strategy;
  use crate::strategy::TupleUnion;
  use crate::strategy::Union;
  use crate::strategy::ValueTree as _;
  use crate::test_runner::TestCaseError;
  use crate::test_runner::TestRunner;
  use crate::test_runner::test_runner_without_persistence;

  /// Ensure a `prop_oneof!` strategy can generate every expected arm.
  fn expect_oneof_count(n: usize, strategy: impl Strategy<Value = i32>) -> Result<(), TestFailure> {
    use std::collections::HashSet;

    let mut runner = test_runner_without_persistence();
    let mut seen = HashSet::new();
    for _ in 0..1024 {
      let tree = match strategy.new_tree(&mut runner) {
        Ok(tree) => tree,
        Err(reason) => {
          return Err(TestFailure::WasErr {
            context: "oneof strategy generates a value tree",
            cause:   reason.message().into(),
          });
        }
      };
      let _was_new = seen.insert(tree.current());
    }

    ensure_eq(&n, &seen.len(), "oneof strategy covers every arm")
  }

  /// Type-check that `prop_oneof!` selected the tuple-union strategy.
  const fn assert_static_oneof<T>(union: TupleUnion<T>) -> TupleUnion<T> {
    union
  }

  prop_compose! {
      /// These are docs!
      fn two_ints(relative: i32)(low in 0..relative, high in relative..)
                 -> (i32, i32) {
          (low, high)
      }
  }

  prop_compose! {
      /// These are docs!
      pub(super) fn two_ints_pub(relative: i32)(low in 0..relative, high in relative..)
                         -> (i32, i32) {
          (low, high)
      }
  }

  prop_compose_ffi! {
      /// These are docs!
      pub(super) fn two_ints_pub_with_ffi_mapper
          (relative: i32)(low in 0..relative, high in relative..)
      with extern "C" fn signed_gap(low: i32, high: i32) -> i32 {
          high.saturating_sub(low)
      }
      call signed_gap(low, high);
  }

  prop_compose_ffi! {
      fn two_stage_ffi_mapper(base: i32)
          (greater in base.saturating_add(1)..base.saturating_add(100))
          (lesser in base..greater, greater in Just(greater))
      with extern "C" fn span(lesser: i32, greater: i32) -> i32 {
          greater.saturating_sub(lesser)
      }
      call span(lesser, greater);
  }

  prop_compose! {
      fn a_less_than_b()(greater in 0..1000)(lesser in 0..greater, greater in Just(greater))
                      -> (i32, i32) {
          (lesser, greater)
      }
  }

  __proptest_internal! {
      #[test]
      fn test_something(first in 0_u32..42_u32, second in 1_u32..10_u32) {
          if first == 41 && second == 9 {
              return Err(TestCaseError::reject(
                  "the rejected edge case is covered by the assumption path",
              ));
          }
          if first.saturating_add(second) >= 50 {
              return Err(TestCaseError::fail(
                  "the generated sum stays below the documented bound",
              ));
          }
      }
  }

  prop_compose! {
      fn single_closure_is_move(base: u64)(off in 0..10_u64) -> u64 {
          base.saturating_add(off)
      }
  }

  prop_compose! {
      fn double_closure_is_move
          (base: u64)
          (off1 in 0..10_u64)
          (off2 in off1..off1.saturating_add(10))
          -> u64
      {
          base.saturating_add(off2)
      }
  }

  mod test_arg_counts {
    use core::hint::black_box;

    use crate::strategy::Just;

    __proptest_internal! {
        #[test]
        fn test_1_arg(first in Just(0)) {
            let observed = black_box([first]);
            let _arity = observed.len();
        }
        #[test]
        fn test_2_arg(first in Just(0), second in Just(0)) {
            let values: [i32; 2] = (first, second).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_3_arg(first in Just(0), second in Just(0), third in Just(0)) {
            let values: [i32; 3] = (first, second, third).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_4_arg(first in Just(0), second in Just(0), third in Just(0),
                      fourth in Just(0)) {
            let values: [i32; 4] =
                (first, second, third, fourth).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_5_arg(first in Just(0), second in Just(0), third in Just(0),
                      fourth in Just(0), fifth in Just(0)) {
            let values: [i32; 5] =
                (first, second, third, fourth, fifth).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_6_arg(first in Just(0), second in Just(0), third in Just(0),
                      fourth in Just(0), fifth in Just(0), f in Just(0)) {
            let values: [i32; 6] =
                (first, second, third, fourth, fifth, f).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_7_arg(first in Just(0), second in Just(0), third in Just(0),
                      fourth in Just(0), fifth in Just(0), f in Just(0),
                      seventh in Just(0)) {
            let values: [i32; 7] =
                (first, second, third, fourth, fifth, f, seventh).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_8_arg(first in Just(0), second in Just(0), third in Just(0),
                      fourth in Just(0), fifth in Just(0), f in Just(0),
                      seventh in Just(0), eighth in Just(0)) {
            let values: [i32; 8] = (
                first, second, third, fourth, fifth, f, seventh, eighth,
            ).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_9_arg(first in Just(0), second in Just(0), third in Just(0),
                      fourth in Just(0), fifth in Just(0), f in Just(0),
                      seventh in Just(0), eighth in Just(0), i in Just(0)) {
            let values: [i32; 9] = (
                first, second, third, fourth, fifth, f, seventh, eighth, i,
            ).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_a_arg(first in Just(0), second in Just(0), third in Just(0),
                      fourth in Just(0), fifth in Just(0), f in Just(0),
                      seventh in Just(0), eighth in Just(0), i in Just(0),
                      j in Just(0)) {
            let values: [i32; 10] = (
                first, second, third, fourth, fifth, f, seventh, eighth, i,
                j,
            ).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_b_arg(first in Just(0), second in Just(0), third in Just(0),
                      fourth in Just(0), fifth in Just(0), f in Just(0),
                      seventh in Just(0), eighth in Just(0), i in Just(0),
                      j in Just(0), eleventh in Just(0)) {
            let values: [i32; 11] = (
                first, second, third, fourth, fifth, f, seventh, eighth, i,
                j, eleventh,
            ).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
        #[test]
        fn test_c_arg(first in Just(0), second in Just(0), third in Just(0),
                      fourth in Just(0), fifth in Just(0), f in Just(0),
                      seventh in Just(0), eighth in Just(0), i in Just(0),
                      j in Just(0), eleventh in Just(0), twelfth in Just(0)) {
            let values: [i32; 12] = (
                first, second, third, fourth, fifth, f, seventh, eighth, i,
                j, eleventh, twelfth,
            ).into();
            let observed = black_box(values);
            let _arity = observed.len();
        }
    }
  }

  fn draw<S: Strategy>(strategy: S) -> Result<S::Value, TestFailure> {
    let mut runner = TestRunner::deterministic();
    Ok(ensure_some(strategy.new_tree(&mut runner).ok(), "strategy generates a value tree")?.current())
  }

  #[test]
  fn prop_compose_fixtures_generate_values() -> Result<(), TestFailure> {
    let (low, high) = draw(two_ints(10))?;
    ensure(low < 10, "lower value honors the first strategy")?;
    ensure(high >= 10, "higher value honors the second strategy")?;

    let (lesser, greater) = draw(a_less_than_b())?;
    ensure(lesser < greater, "dependent strategy keeps lesser below greater")?;

    let single = draw(single_closure_is_move(10))?;
    ensure((10..20).contains(&single), "single closure strategy captures the base argument")?;

    let ffi_gap = draw(two_ints_pub_with_ffi_mapper(10))?;
    ensure(ffi_gap > 0, "one-layer ffi mapper receives generated scalar values")?;

    let ffi_span = draw(two_stage_ffi_mapper(10))?;
    ensure(ffi_span > 0, "two-layer ffi mapper receives dependent generated scalar values")?;

    let double = draw(double_closure_is_move(10))?;
    ensure(
      (10..29).contains(&double),
      "double closure strategy captures the base argument through both stages",
    )
  }

  #[test]
  fn named_arguments_is_debug_for_needed_cases() -> Result<(), TestFailure> {
    use super::NamedArguments;

    ensure_eq(
      &std::format!("{:?}", NamedArguments("foo", &"bar")),
      &"foo = \"bar\"".to_owned(),
      "single named value keeps the name/value format",
    )?;

    let one = std::format!("{:?}", NamedArguments(("foo",), &(1,)));
    ensure_eq(&one, &"foo = 1".to_owned(), "one tuple argument formats without tuple punctuation")?;
    ensure(!one.contains(','), "one tuple argument formatting does not contain a comma")?;

    ensure_eq(
      &std::format!("{:?}", NamedArguments(("foo", "bar"), &(1, 2))),
      &"foo = 1, bar = 2".to_owned(),
      "two tuple arguments are comma separated",
    )?;

    drop(std::format!("{:?}", NamedArguments(("a", "b", "c"), &(1, 2, 3))));
    drop(std::format!("{:?}", NamedArguments(("a", "b", "c", "d"), &(1, 2, 3, 4))));
    drop(std::format!("{:?}", NamedArguments(("a", "b", "c", "d", "e"), &(1, 2, 3, 4, 5))));
    drop(std::format!(
      "{:?}",
      NamedArguments(("a", "b", "c", "d", "e", "f"), &(1, 2, 3, 4, 5, 6))
    ));
    drop(std::format!(
      "{:?}",
      NamedArguments(("a", "b", "c", "d", "e", "f", "g"), &(1, 2, 3, 4, 5, 6, 7))
    ));
    drop(std::format!(
      "{:?}",
      NamedArguments(("a", "b", "c", "d", "e", "f", "g", "h"), &(1, 2, 3, 4, 5, 6, 7, 8))
    ));
    drop(std::format!(
      "{:?}",
      NamedArguments(("a", "b", "c", "d", "e", "f", "g", "h", "i"), &(1, 2, 3, 4, 5, 6, 7, 8, 9))
    ));
    drop(std::format!(
      "{:?}",
      NamedArguments(("a", "b", "c", "d", "e", "f", "g", "h", "i", "j"), &(1, 2, 3, 4, 5, 6, 7, 8, 9, 10))
    ));
    drop(std::format!("{:?}", NamedArguments((("a", "b"), "c", "d"), &((1, 2), 3, 4))));
    Ok(())
  }

  #[test]
  fn oneof_static_counts_through_five() -> Result<(), TestFailure> {
    expect_oneof_count(1, prop_oneof![Just(0_i32)])?;
    expect_oneof_count(2, assert_static_oneof(prop_oneof![Just(0_i32), Just(1_i32),]))?;
    expect_oneof_count(3, assert_static_oneof(prop_oneof![Just(0_i32), Just(1_i32), Just(2_i32),]))?;
    expect_oneof_count(
      4,
      assert_static_oneof(prop_oneof![Just(0_i32), Just(1_i32), Just(2_i32), Just(3_i32),]),
    )?;
    expect_oneof_count(
      5,
      assert_static_oneof(prop_oneof![Just(0_i32), Just(1_i32), Just(2_i32), Just(3_i32), Just(4_i32),]),
    )
  }

  #[test]
  fn oneof_static_counts_six_through_ten() -> Result<(), TestFailure> {
    expect_oneof_count(
      6,
      assert_static_oneof(prop_oneof![
        Just(0_i32),
        Just(1_i32),
        Just(2_i32),
        Just(3_i32),
        Just(4_i32),
        Just(5_i32),
      ]),
    )?;
    expect_oneof_count(
      7,
      assert_static_oneof(prop_oneof![
        Just(0_i32),
        Just(1_i32),
        Just(2_i32),
        Just(3_i32),
        Just(4_i32),
        Just(5_i32),
        Just(6_i32),
      ]),
    )?;
    expect_oneof_count(
      8,
      assert_static_oneof(prop_oneof![
        Just(0_i32),
        Just(1_i32),
        Just(2_i32),
        Just(3_i32),
        Just(4_i32),
        Just(5_i32),
        Just(6_i32),
        Just(7_i32),
      ]),
    )?;
    expect_oneof_count(
      9,
      assert_static_oneof(prop_oneof![
        Just(0_i32),
        Just(1_i32),
        Just(2_i32),
        Just(3_i32),
        Just(4_i32),
        Just(5_i32),
        Just(6_i32),
        Just(7_i32),
        Just(8_i32),
      ]),
    )?;
    expect_oneof_count(
      10,
      assert_static_oneof(prop_oneof![
        Just(0_i32),
        Just(1_i32),
        Just(2_i32),
        Just(3_i32),
        Just(4_i32),
        Just(5_i32),
        Just(6_i32),
        Just(7_i32),
        Just(8_i32),
        Just(9_i32),
      ]),
    )
  }

  #[test]
  fn oneof_dynamic_count_after_tuple_limit() -> Result<(), TestFailure> {
    let dynamic_oneof: Union<_> = prop_oneof![
      Just(0_i32),
      Just(1_i32),
      Just(2_i32),
      Just(3_i32),
      Just(4_i32),
      Just(5_i32),
      Just(6_i32),
      Just(7_i32),
      Just(8_i32),
      Just(9_i32),
      Just(10_i32),
    ];
    expect_oneof_count(11, dynamic_oneof)
  }
}

#[cfg(test)]
#[cfg(feature = "timeout")]
mod test_timeout {
  use crate::test_runner::Config;
  use crate::test_runner::runner_test_config;

  __proptest_internal! {
      #![proptest_config(Config {
          fork: true,
          .. runner_test_config()
      })]

      // Ensure that the macro sets the test name properly. If it doesn't,
      // this test will fail to run correctly.
      #[test]
      fn test_name_set_correctly_for_fork(_ in 0_u32..1_u32) { }
  }
}

#[cfg(test)]
mod another_test {
  use crate::sugar;

  // Ensure that we can access the `[pub]` composed function above.
  #[test]
  fn can_access_pub_compose() {
    drop(sugar::test::two_ints_pub(42));
    drop(sugar::test::two_ints_pub_with_ffi_mapper(42));
  }
}

#[cfg(test)]
mod ownership_tests {
  use core::hint::black_box;

  #[cfg(feature = "std")]
  __proptest_internal! {
      #[test]
      fn accept_ref_arg(ref digit in "[0-9]") {
          use crate::std_facade::String;
          fn assert_string(_s: &String) {}
          assert_string(digit);
      }

      #[test]
      fn accept_move_arg(digit in "[0-9]") {
          use crate::std_facade::String;
          fn assert_string(_s: String) {}
          assert_string(digit);
      }
  }

  #[derive(Debug)]
  struct NotClone;
  const MK: fn() -> NotClone = || NotClone;

  __proptest_internal! {
      #[test]
      fn accept_noclone_arg(nc in MK) {
          let _: NotClone = black_box(nc);
      }

      #[test]
      fn accept_noclone_ref_arg(ref nc in MK) {
          let _: &NotClone = black_box(nc);
      }
  }
}

#[cfg(test)]
mod closure_tests {
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;

  use crate::test_runner::TestCaseError;
  use crate::test_runner::runner_test_config;

  #[test]
  fn test_simple() -> Result<(), TestFailure> {
    let x = 420;

    ensure(
      __proptest_internal!(|(y: i32)| {
          let _: (i32, i32) = (x, y);
      })
      .is_ok(),
      "typed closure-style syntax runs",
    )?;

    ensure(
      __proptest_internal!(|(y in 0..100)| {
          let _: (i32, i32) = (x, y);
      })
      .is_ok(),
      "strategy closure-style syntax runs",
    )?;

    ensure(
      __proptest_internal!(|(y: i32,)| {
          let _: (i32, i32) = (x, y);
      })
      .is_ok(),
      "typed closure-style syntax accepts a trailing comma",
    )?;

    ensure(
      __proptest_internal!(|(y in 0..100,)| {
          let _: (i32, i32) = (x, y);
      })
      .is_ok(),
      "strategy closure-style syntax accepts a trailing comma",
    )
  }

  #[test]
  fn test_move() -> Result<(), TestFailure> {
    #[derive(Debug)]
    struct Foo;

    let first_foo = Foo;

    ensure(
      __proptest_internal!(move |(x in 1_i32..100_i32, y in 0_i32..100_i32)| {
          let _: (i32, &Foo) = (x.saturating_add(y), &first_foo);
      })
      .is_ok(),
      "move closure captures surrounding state",
    )?;

    let second_foo = Foo;
    ensure(
      __proptest_internal!(move |(x: (), y: ())| {
          fn accept_units(_: (), _: ()) -> usize {
              2
          }

          let _: (usize, &Foo) = (accept_units(x, y), &second_foo);
      })
      .is_ok(),
      "typed move closure captures surrounding state",
    )
  }

  #[test]
  fn returns_error_if_closure_fails() -> Result<(), TestFailure> {
    let result = __proptest_internal!(|(_ in 0..1)| {
        let should_fail = true;
        if should_fail {
            return Err(TestCaseError::fail(
                "intentional test-case failure",
            ));
        }
    });
    ensure(result.is_err(), "closure-style failure is returned")
  }

  #[test]
  fn accepts_unblocked_syntax() -> Result<(), TestFailure> {
    ensure(
      __proptest_internal!(|(x in 0_u32..10, y in 10_u32..20)| {
          let _: (u32, u32) = (x, y);
      })
      .is_ok(),
      "closure-style syntax accepts two generated values",
    )?;
    ensure(
      __proptest_internal!(|(x in 0_u32..10, y in 10_u32..20,)| {
          let _: (u32, u32) = (x, y);
      })
      .is_ok(),
      "closure-style syntax accepts a trailing comma",
    )
  }

  #[test]
  fn accepts_custom_config() -> Result<(), TestFailure> {
    let conf = runner_test_config();

    ensure(
      __proptest_internal!(conf, |(x in 0_u32..10, y in 10_u32..20)| {
          let _: (u32, u32) = (x, y);
      })
      .is_ok(),
      "owned custom config is accepted",
    )?;
    ensure(
      __proptest_internal!(&conf, |(x in 0_u32..10, y in 10_u32..20)| {
          let _: (u32, u32) = (x, y);
      })
      .is_ok(),
      "borrowed custom config is accepted",
    )?;
    ensure(
      __proptest_internal!(conf, move |(x in 0_u32..10, y in 10_u32..20)| {
          let _: (u32, u32) = (x, y);
      })
      .is_ok(),
      "move closure accepts an owned custom config",
    )?;
    ensure(
      __proptest_internal!(conf, |(_x: u32, _y: u32)| {}).is_ok(),
      "typed closure accepts an owned custom config",
    )?;
    ensure(
      __proptest_internal!(conf, move |(_x: u32, _y: u32)| {}).is_ok(),
      "typed move closure accepts an owned custom config",
    )?;

    // Same as above, but with extra trailing comma
    ensure(
      __proptest_internal!(conf, |(x in 0_u32..10, y in 10_u32..20,)| {
          let _: (u32, u32) = (x, y);
      })
      .is_ok(),
      "owned custom config accepts a trailing comma",
    )?;
    ensure(
      __proptest_internal!(&conf, |(x in 0_u32..10, y in 10_u32..20,)| {
          let _: (u32, u32) = (x, y);
      })
      .is_ok(),
      "borrowed custom config accepts a trailing comma",
    )?;
    ensure(
      __proptest_internal!(conf, move |(x in 0_u32..10, y in 10_u32..20,)| {
          let _: (u32, u32) = (x, y);
      })
      .is_ok(),
      "move closure with custom config accepts a trailing comma",
    )?;
    ensure(
      __proptest_internal!(conf, |(_x: u32, _y: u32,)| {}).is_ok(),
      "typed closure with custom config accepts a trailing comma",
    )?;
    ensure(
      __proptest_internal!(conf, move |(_x: u32, _y: u32,)| {}).is_ok(),
      "typed move closure with custom config accepts a trailing comma",
    )
  }
}

#[cfg(test)]
mod any_tests {
  use strict_test_support::TestFailure;

  __proptest_internal! {
      #[test]
      fn test_something
          (
              flag: bool,
              first in 25_u8..,
              second in 25_u8..,
              _d: (),
              mut _e: (),
              ref _f: (),
              ref mut _g: (),
              [(), ()]: [(); 2],
          ) {
          let _: bool = flag;
          let _sum = usize::from(first).saturating_add(usize::from(second));
      }
  }

  // Test that the macro accepts some of the inputs we expect it to:
  #[test]
  fn proptest_ext_test() -> Result<(), TestFailure> {
    use strict_test_support::ensure_eq;

    struct Wrapper(pub u8);

    fn accept_strategy<T>(_strategy: T) {}

    accept_strategy(proptest_helper!(@_EXT _STRAT( _ : u8 )));
    accept_strategy(proptest_helper!(@_EXT _STRAT( x : u8 )));
    accept_strategy(proptest_helper!(@_EXT _STRAT( ref x : u8 )));
    accept_strategy(proptest_helper!(@_EXT _STRAT( mut x : u8 )));
    accept_strategy(proptest_helper!(@_EXT _STRAT( ref mut x : u8 )));
    accept_strategy(proptest_helper!(@_EXT _STRAT( [_, _] : u8 )));
    accept_strategy(proptest_helper!(@_EXT _STRAT( (&mut Wrapper(x)) : u8 )));
    accept_strategy(proptest_helper!(@_EXT _STRAT( x in 1..2 )));

    let proptest_helper!(@_EXT _PAT( _ : u8 )): u8 = 1;
    let proptest_helper!(@_EXT _PAT( _name : u8 )) = 1;
    let proptest_helper!(@_EXT _PAT( mut _mut_name : u8 )) = 1;
    let proptest_helper!(@_EXT _PAT( [_, _] : u8 )) = [1, 2];
    let proptest_helper!(@_EXT _PAT( (&mut Wrapper(_wrapped)) : u8 )) = &mut Wrapper(1);
    let proptest_helper!(@_EXT _PAT( ranged in 1..2 )) = 1;
    ensure_eq(&ranged, &1, "ranged pattern binds the generated value")?;
    let matched_ref = u8::from(matches!(Some(1), Some(proptest_helper!(@_EXT _PAT( ref _x : u8 )))));
    ensure_eq(&matched_ref, &1, "ref pattern matches the generated value")?;

    let matched_ref_mut = u8::from(matches!(Some(1), Some(proptest_helper!(@_EXT _PAT( ref mut _x : u8 )))));
    ensure_eq(&matched_ref_mut, &1, "ref mut pattern matches the generated value")
  }
}

// Behavioural coverage for the `macro_rules!` hygiene above: the general
// `prop_oneof!` arm that builds a dynamic `Union::new_weighted` and the
// two-closure-list `prop_compose!` rule that routes typed argument lists
// through the `@_EXT` machinery. Both transcribers were realigned from `*`
// to `+`, and the two-list `prop_compose!` rule additionally had an unbound
// `$strategy` metavariable replaced with the intended `@_EXT _STRAT` call, so
// this module drives each form end to end through the strict runner: a
// regression surfaces as a returned `TestFailure`, never a panic.
#[cfg(test)]
#[cfg(feature = "strict-test")]
mod macro_hygiene {
  use std::string::ToString as _;

  use strict_test_support::ensure;
  use strict_test_support::ensure_contains;
  use strict_test_support::ensure_some;

  use crate::strategy::Just;
  use crate::strategy::Strategy;
  use crate::strict::TestResult;
  use crate::strict::ensure_property;

  // Eleven arms with no trailing comma drives the general `prop_oneof!`
  // arm whose transcriber previously repeated `$weight`/`$item` under
  // `,*`; it expands to a dynamic `Union::new_weighted`. The arms carry
  // distinct values so a dropped arm would show up as a missing sample.
  fn eleven_way_union() -> impl Strategy<Value = i32> {
    // Unsuffixed literals infer to `i32` from the return type.
    prop_oneof![
      Just(0),
      Just(1),
      Just(2),
      Just(3),
      Just(4),
      Just(5),
      Just(6),
      Just(7),
      Just(8),
      Just(9),
      Just(10)
    ]
  }

  #[test]
  fn prop_oneof_dynamic_union_stays_within_its_arms() -> TestResult {
    ensure_property(&eleven_way_union(), "every sample comes from one of the eleven arms", |sample| {
      ensure((0..=10).contains(&sample), "sample in 0..=10")
    })
  }

  #[test]
  fn prop_oneof_dynamic_union_reports_a_falsified_bound() -> TestResult {
    let failure = ensure_some(
      ensure_property(&eleven_way_union(), "no sample reaches the eleventh arm", |sample| {
        ensure(sample < 10, "sample below ten")
      })
      .err(),
      "the eleventh arm must falsify the below-ten property",
    )?;
    ensure_contains(
      &failure.to_string(),
      "property falsified",
      "the falsification surfaces through the strict runner",
    )
  }

  // `prop_compose!` with two closure lists where the first list uses the
  // `name: type` form routes through the `$($arg:tt)+`/`$($arg2:tt)+`
  // rule — the arm that previously transcribed the unbound `$strategy`
  // metavariable. The first stage draws a ceiling via `any::<u8>()`; the
  // second stage draws a value bounded by that ceiling and threads the
  // ceiling back out so the body can return both.
  prop_compose! {
      fn ceiling_then_bounded()
          (ceiling: u8)
          (drawn in 0_u8..=ceiling, ceiling_kept in Just(ceiling))
          -> (u8, u8)
      {
          (drawn, ceiling_kept)
      }
  }

  #[test]
  fn prop_compose_typed_two_stage_bounds_hold() -> TestResult {
    ensure_property(
      &ceiling_then_bounded(),
      "the second-stage draw never exceeds the first-stage ceiling",
      |(drawn, ceiling)| ensure(drawn <= ceiling, "draw within the drawn ceiling"),
    )
  }

  #[test]
  fn prop_compose_typed_two_stage_reports_falsification() -> TestResult {
    let failure = ensure_some(
      ensure_property(
        &ceiling_then_bounded(),
        "the second draw never equals the ceiling",
        |(drawn, ceiling)| ensure(drawn != ceiling, "draw differs from the ceiling"),
      )
      .err(),
      "a draw equal to the ceiling must falsify the property",
    )?;
    ensure_contains(
      &failure.to_string(),
      "property falsified",
      "the falsification surfaces through the strict runner",
    )
  }
}
