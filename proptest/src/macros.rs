//-
// Copyright 2017, 2018 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Macros for internal use to reduce boilerplate.

// Pervasive internal sugar
/// Defines a unit struct implementing `strategy::statics::MapFn`.
///
/// Wraps a named mapping-function body as a zero-sized `MapFn` type so it can
/// parameterise a `statics::Map` without capturing a closure, keeping the
/// resulting strategy `Clone`/`Copy`/`Debug`.
macro_rules! mapfn {
    ($({#[$allmeta:meta]})* $(#[$meta:meta])* [$($vis:tt)*]
     fn $name:ident[$($gen:tt)*]($parm:ident: $input:ty) -> $output:ty {
         $($body:tt)*
     }) => {
        $(#[$allmeta])* $(#[$meta])*
        #[derive(Clone, Copy, Debug)]
        $($vis)* struct $name;
        $(#[$allmeta])*
        impl $($gen)* $crate::strategy::statics::MapFn<$input> for $name {
            type Output = $output;
            fn apply(&self, $parm: $input) -> $output {
                $($body)*
            }
        }
    }
}

/// Emits the three `ValueTree` methods that forward to the wrapped tree held
/// at tuple position `0`.
///
/// Used by newtype `ValueTree` wrappers whose only field is the inner tree, so
/// `current`/`simplify`/`complicate` simply delegate to it.
macro_rules! delegate_vt_0 {
  () => {
    fn current(&self) -> Self::Value {
      self.0.current()
    }

    fn simplify(&mut self) -> bool {
      self.0.simplify()
    }

    fn complicate(&mut self) -> bool {
      self.0.complicate()
    }
  };
}

/// Generates the `Strategy` + `ValueTree` newtype boilerplate for an opaque
/// wrapper around an inner strategy.
///
/// Declares the strategy and value-tree structs, forwards `new_tree` to the
/// inner strategy (mapping its tree into the wrapper), and delegates the
/// value-tree methods via `delegate_vt_0!`. Used by `option`/`result`/
/// `collection`/`sample`/`string` to hide their inner combinator types.
macro_rules! opaque_strategy_wrapper {
    ($({#[$allmeta:meta]})*
     $(#[$smeta:meta])*
     pub struct $stratname:ident
     [$($sgen:tt)*][$($swhere:tt)*]
     ($innerstrat:ty) -> $stratvtty:ty;

     $(#[$vmeta:meta])* pub struct $vtname:ident
     [$($vgen:tt)*][$($vwhere:tt)*]
     ($innervt:ty) -> $actualty:ty;
    ) => {
        $(#[$allmeta])*
        $(#[$smeta])*
        #[must_use = "strategies do nothing unless used"]
        pub struct $stratname $($sgen)* ($innerstrat)
            $($swhere)*;

        $(#[$allmeta])*
        $(#[$vmeta])* pub struct $vtname $($vgen)* ($innervt) $($vwhere)*;

        $(#[$allmeta])*
        impl $($sgen)* Strategy for $stratname $($sgen)* $($swhere)* {
            type Tree = $stratvtty;
            type Value = $actualty;
            fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
                self.0.new_tree(runner).map($vtname)
            }
        }

        $(#[$allmeta])*
        impl $($vgen)* ValueTree for $vtname $($vgen)* $($vwhere)* {
            type Value = $actualty;

            delegate_vt_0!();
        }
    }
}

/// Unwraps a `Result`, evaluating a fallback expression on `Err`.
///
/// Binds the error to the given identifier for use in the fallback, e.g.
/// `unwrap_or!(result, err => handle_err(err))`. Unlike `Result::unwrap_or`
/// the fallback can reference the error and may diverge (`return`/`continue`).
macro_rules! unwrap_or {
  ($unwrap:expr, $err:ident => $on_err:expr) => {
    match $unwrap {
      Ok(ok) => ok,
      Err($err) => $on_err,
    }
  };
}
