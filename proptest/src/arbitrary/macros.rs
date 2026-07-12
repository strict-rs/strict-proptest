//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//==============================================================================
// Macros for quick implementing:
//==============================================================================

/// Writes an `impl Arbitrary` for a type in a single line.
///
/// The full form `arbitrary!([bounds] T, Strat, Params; args => expr)` fixes
/// the `Strategy` and `Parameters` types and gives the body access to the
/// generated params; shorter forms default `Params` to `()` or wrap a constant
/// value in `Just<Self>`. The list form `arbitrary!(A, B, ...)` expands each
/// name to `arbitrary!(T, T::Any; T::ANY)`, used for the bool and integer
/// primitives.
macro_rules! arbitrary {
    ([$($bounds : tt)*] $typ: ty, $strat: ty, $params: ty;
        $args: ident => $logic: expr) => {
        impl<$($bounds)*> $crate::arbitrary::Arbitrary for $typ {
            type Parameters = $params;
            type Strategy = $strat;
            fn arbitrary_with($args: Self::Parameters) -> Self::Strategy {
                $logic
            }
        }
    };
    ([$($bounds : tt)*] $typ: ty, $strat: ty; $logic: expr) => {
        arbitrary!([$($bounds)*] $typ, $strat, (); _args => $logic);
    };
    ([$($bounds : tt)*] $typ: ty; $logic: expr) => {
        arbitrary!([$($bounds)*] $typ,
            $crate::strategy::Just<Self>, ();
            _args => $crate::strategy::Just($logic)
        );
    };
    ($typ: ty, $strat: ty; $logic: expr) => {
        arbitrary!([] $typ, $strat; $logic);
    };
    ($strat: ty; $logic: expr) => {
        arbitrary!([] $strat; $logic);
    };
    ($($typ: ident),*) => {
        $(arbitrary!($typ, $typ::Any; $typ::ANY);)*
    };
}

/// Implements `Arbitrary` for a newtype `W<A>` built by passing an arbitrary
/// `A` through a constructor.
///
/// The value is drawn from `any::<A>()` and mapped through the explicit
/// constructor with `static_map`, giving `Strategy = SMapped<A, Self>`; a
/// matching `lift1!` impl is emitted so the newtype can also be lifted.
macro_rules! wrap_ctor {
    ($wrap: ident, $maker: expr) => {
        wrap_ctor!([] $wrap, $maker);
    };
    ([$($bound : tt)*] $wrap: ident, $maker: expr) => {
        arbitrary!([A: $crate::arbitrary::Arbitrary + $($bound)*] $wrap<A>,
            $crate::arbitrary::SMapped<A, Self>, A::Parameters;
            args => $crate::strategy::statics::static_map(
                $crate::arbitrary::any_with::<A>(args), $maker));

        lift1!([$($bound)*] $wrap<A>; $maker);
    };
}

/// Implements `Arbitrary` for a wrapper `W<A>` built from an arbitrary `A`
/// via `From`/`Into`.
///
/// Reuses `A`'s own strategy, mapping it with `prop_map_into` to give
/// `Strategy = MapInto<A::Strategy, Self>`, and emits the companion `lift1!`
/// impl. Used for wrappers such as `Box`, `Rc`, and `Arc`.
macro_rules! wrap_from {
    ($wrap: ident) => {
        wrap_from!([] $wrap);
    };
    ([$($bound : tt)*] $wrap: ident) => {
        arbitrary!([A: $crate::arbitrary::Arbitrary + $($bound)*] $wrap<A>,
            $crate::strategy::MapInto<A::Strategy, Self>, A::Parameters;
            args => $crate::strategy::Strategy::prop_map_into(
                $crate::arbitrary::any_with::<A>(args)));

        lift1!([$($bound)*] $wrap<A>);
    };
}

/// Implements `Arbitrary` for types whose value is produced by a
/// zero-argument function at generation time.
///
/// Each `Type, f` pair becomes an impl with
/// `Strategy = LazyJust<Self, fn() -> Self>`, deferring construction until a
/// value is drawn rather than capturing it in a constant. Suited to values
/// that cannot be built in a `const` initializer.
macro_rules! lazy_just {
    ($($self: ty, $fun: expr);+) => {
        $(
            arbitrary!($self, $crate::strategy::LazyJust<Self, fn() -> Self>;
                $crate::strategy::LazyJust::new($fun));
        )+
    };
}

//==============================================================================
// Macros for testing:
//==============================================================================

/// We are mostly interested in ensuring that generating input from our
/// strategies is able to construct a value, therefore ensuring that
/// no panic occurs is mostly sufficient. Shrinking for strategies that
/// use special shrinking methods can be handled separately.
#[cfg(all(test, feature = "strict-test"))]
macro_rules! no_panic_test {
    ($($name: ident => $self: ty),+ $(,)?) => {
        $(
            #[test]
            fn $name() -> $crate::strict::TestResult {
                $crate::strict::ensure_property(
                    &$crate::arbitrary::any::<$self>(),
                    concat!(module_path!(), "::", stringify!($name)),
                    |_| Ok(()),
                )
            }
        )+
    };
}

#[cfg(all(test, not(feature = "strict-test")))]
macro_rules! no_panic_test {
  ($($name:ident => $self:ty),+ $(,)?) => {
      $(
          #[test]
          fn $name() -> ::core::result::Result<(), ::strict_test_support::TestFailure> {
              use $crate::strategy::Strategy as _;

              let mut runner = $crate::test_runner::TestRunner::deterministic();
              ::strict_test_support::ensure(
                  $crate::arbitrary::any::<$self>().new_tree(&mut runner).is_ok(),
                  concat!(module_path!(), "::", stringify!($name)),
              )
          }
      )+
  };
}
