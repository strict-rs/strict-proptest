//-
// Copyright 2024 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(feature = "handle-panics")]
mod internal {
  //! Thread-scoped suppression of the process panic hook.
  //!
  //! The first call to `suppress_panic_hook` installs a dispatching panic
  //! hook process-wide and records the hook it replaced. While a thread has
  //! suppression active the dispatcher drops the panic report for panics
  //! raised on that thread; otherwise it forwards to the recorded hook. This
  //! lets the runner silence the intermediate backtraces printed while
  //! shrinking without touching the unwind (the panic is still caught by
  //! `catch_unwind` in the runner) and without affecting panics on any other
  //! thread or outside a scope.
  use std::boxed::Box;
  use std::cell::Cell;
  use std::panic::PanicHookInfo;
  use std::panic::set_hook;
  use std::panic::take_hook;
  use std::sync::OnceLock;
  use std::thread_local;

  thread_local! {
      /// Whether panic reports raised on the current thread are currently
      /// being suppressed. Held `true` only for the duration of a
      /// `suppress_panic_hook` body.
      static SUPPRESSED: Cell<bool> = const { Cell::new(false) };
  }

  /// Boxed process panic hook stored before the dispatcher is installed.
  type PanicHook = Box<dyn for<'a> Fn(&PanicHookInfo<'a>) + Send + Sync>;

  /// The panic hook that was installed before this module took over. The
  /// dispatcher forwards to it whenever suppression is inactive. Populated
  /// exactly once, when the dispatching hook is installed.
  static PREVIOUS_HOOK: OnceLock<PanicHook> = OnceLock::new();

  /// Installs the process-global dispatching panic hook on first use,
  /// recording the hook it replaces so the dispatcher can forward to it.
  /// Idempotent: later calls observe the already-initialized cell and do
  /// nothing.
  #[allow(
    clippy::single_call_fn,
    reason = "one-time installer of the dispatching panic hook, invoked only from suppress_panic_hook"
  )]
  fn install_dispatcher() {
    let _previous = PREVIOUS_HOOK.get_or_init(|| {
      let previous = take_hook();
      set_hook(Box::new(dispatch));
      previous
    });
  }

  /// Process-global panic hook. Forwards to the previously installed hook,
  /// unless the panicking thread has suppression active, in which case the
  /// report is dropped without interrupting the unwind.
  #[allow(
    clippy::single_call_fn,
    reason = "the process panic-hook body, named only so it can be registered once with set_hook"
  )]
  fn dispatch(panic_info: &PanicHookInfo<'_>) {
    if SUPPRESSED.get() {
      return;
    }
    if let Some(previous) = PREVIOUS_HOOK.get() {
      previous(panic_info);
    }
  }

  /// Restores the current thread's suppression flag to a saved value when
  /// dropped, so the flag is reset even if the guarded body unwinds.
  struct RestoreSuppression(bool);

  impl Drop for RestoreSuppression {
    fn drop(&mut self) {
      SUPPRESSED.set(self.0);
    }
  }

  /// Runs `body` with panic reports raised on the current thread suppressed.
  ///
  /// A panic inside `body` still unwinds normally (and is caught by the
  /// caller); only its stderr report is silenced. The previous flag value is
  /// restored on the way out, including on unwind, so nested scopes and
  /// other threads are unaffected.
  ///
  /// # Returns
  /// `body`'s return value.
  #[allow(
    clippy::single_call_fn,
    reason = "the scoped-suppression entry point, called only from the runner's per-case call_test"
  )]
  pub(in crate::test_runner) fn suppress_panic_hook<R>(body: impl FnOnce() -> R) -> R {
    install_dispatcher();
    let previous = SUPPRESSED.replace(true);
    let _restore = RestoreSuppression(previous);
    body()
  }

  #[cfg(test)]
  mod test {
    use std::cell::Cell;

    use strict_test_support::TestFailure;
    use strict_test_support::ensure;

    use super::SUPPRESSED;
    use super::suppress_panic_hook;

    #[test]
    fn returns_body_value_and_clears_suppression() -> Result<(), TestFailure> {
      let produced = suppress_panic_hook(|| 7_u8);
      ensure(produced == 7, "the body's return value passes through")?;
      ensure(!SUPPRESSED.get(), "suppression is cleared once the scope ends")
    }

    #[test]
    fn nested_scopes_restore_the_outer_flag() -> Result<(), TestFailure> {
      let active_inside = Cell::new(false);
      let active_after_inner = Cell::new(false);
      suppress_panic_hook(|| {
        active_inside.set(SUPPRESSED.get());
        suppress_panic_hook(|| ());
        active_after_inner.set(SUPPRESSED.get());
      });
      ensure(active_inside.get(), "suppression is active while the scope runs")?;
      ensure(active_after_inner.get(), "an inner scope restores the outer scope's suppression")?;
      ensure(!SUPPRESSED.get(), "suppression is cleared once the outer scope ends")
    }
  }
}

#[cfg(not(feature = "handle-panics"))]
mod internal {
  //! No-op suppression used when `handle-panics` is disabled: the body runs
  //! unchanged and the default panic hook still prints.

  /// Runs `body` unchanged; panic reports are not suppressed.
  ///
  /// # Returns
  /// `body`'s return value.
  #[allow(
    clippy::single_call_fn,
    reason = "the handle-panics-off no-op entry point, called only from the runner's per-case call_test"
  )]
  pub(in crate::test_runner) fn suppress_panic_hook<R>(body: impl FnOnce() -> R) -> R {
    body()
  }
}

pub(super) use internal::suppress_panic_hook;
