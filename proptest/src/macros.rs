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

/// Forward the three `ValueTree` methods to a named or tuple field.
///
/// Filter adapters can require recovery after a successful source step.
/// Recovery is never called when the source reports no change.
macro_rules! delegate_value_tree {
  ($field:tt $(, $recover:ident)?) => {
    fn current(&self) -> Self::Value {
      self.$field.current()
    }

    fn simplify(&mut self) -> bool {
      self.$field.simplify() $(&& self.$recover())?
    }

    fn complicate(&mut self) -> bool {
      self.$field.complicate() $(&& self.$recover())?
    }
  };
}

/// Generates the `Strategy` + `ValueTree` newtype boilerplate for an opaque
/// wrapper around an inner strategy.
///
/// Declares the strategy and value-tree structs, forwards `new_tree` to the
/// inner strategy (mapping its tree into the wrapper), and delegates the
/// value-tree methods via `delegate_value_tree!`. Used by `option`/`result`/
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

            delegate_value_tree!(0);
        }
    }
}

/// Implement structured `Debug` with explicit bounds and field expressions.
///
/// Strategy adapters can render their source while keeping function fields
/// opaque, without adding `Debug` bounds to closures or unrelated parameters.
/// Field expressions retain each adapter's existing order and placeholders.
macro_rules! impl_debug_struct {
  ($name:ident<$($generic:ident),+> [$($bounds:tt)*] |$this:ident| {
    $($field:ident: $value:expr),+ $(,)?
  }) => {
    impl<$($generic),+> $crate::std_facade::fmt::Debug for $name<$($generic),+>
    where
      $($bounds)*
    {
      fn fmt(&$this, formatter: &mut $crate::std_facade::fmt::Formatter<'_>) -> $crate::std_facade::fmt::Result {
        formatter.debug_struct(stringify!($name))
          $(.field(stringify!($field), &$value))+
          .finish()
      }
    }
  };
}

/// Clone an adapter's source and share its function without requiring `F: Clone`.
/// Additional state follows the adapter's explicit field initialization rules.
macro_rules! impl_clone_shared_fn {
  ($name:ident<$source:ident, $function:ident> |$this:ident| {
    $($field:ident: $value:expr),* $(,)?
  }) => {
    impl<$source: Clone, $function> Clone for $name<$source, $function> {
      fn clone(&$this) -> Self {
        Self {
          source: $this.source.clone(),
          fun: $crate::std_facade::Arc::clone(&$this.fun),
          $($field: $value,)*
        }
      }
    }
  };
}

/// Unwraps a `Result`, evaluating a fallback expression on `Err`.
///
/// Binds the error to the given identifier for use in the fallback, e.g.
/// `unwrap_or!(result, err => handle_err(err))`. Unlike `Result::unwrap_or`
/// the fallback can reference the error and may diverge (`return`/`continue`).
#[cfg(feature = "std")]
macro_rules! unwrap_or {
  ($unwrap:expr, $err:ident => $on_err:expr) => {
    match $unwrap {
      Ok(ok) => ok,
      Err($err) => $on_err,
    }
  };
}
