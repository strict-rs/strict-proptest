//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::sync`.

use std::fmt;
use std::sync::Arc;
use std::sync::Barrier;
use std::sync::BarrierWaitResult;
use std::sync::Once;
use std::sync::mpsc::IntoIter;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvError;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::mpsc::SendError;
use std::sync::mpsc::Sender;
use std::sync::mpsc::SyncSender;
use std::sync::mpsc::TryRecvError;
use std::sync::mpsc::TrySendError;
use std::sync::mpsc::channel;
use std::sync::mpsc::sync_channel;
use std::thread;

use crate::arbitrary::Arbitrary;
use crate::arbitrary::SMapped;
use crate::arbitrary::any;
use crate::arbitrary::any_with;
use crate::strategy::Just;
use crate::strategy::LazyJust;
use crate::strategy::LazyJustFn;
use crate::strategy::TupleUnion;
use crate::strategy::WeightedStrategy;
use crate::strategy::statics::static_map;

// OnceState can not escape Once::call_once_force.
// PoisonError depends implicitly on the lifetime on MutexGuard, etc.
// This transitively applies to TryLockError.

// Not doing Weak because .upgrade() would always return None.

// Mutex, RwLock, and Condvar are deliberately absent: the strict lint
// policy bans the poisoning-prone std locks outright (clippy.toml
// disallowed-types), so this crate offers no `Arbitrary` for them.
// WaitTimeoutResult is likewise absent — it can only be produced by a real
// `Condvar::wait_timeout` call, which needs those banned locks plus
// panicking lock/unwrap internals.

arbitrary!(Barrier, SMapped<u16, Self>;  // usize would be extreme!
    static_map(any::<u16>(), |n| Barrier::new(usize::from(n)))
);

arbitrary!(BarrierWaitResult,
    TupleUnion<(WeightedStrategy<LazyJustFn<Self>>, WeightedStrategy<LazyJustFn<Self>>)>;
    prop_oneof![LazyJust::new(bwr_true), LazyJust::new(bwr_false)]
);

lazy_just!(Once, Once::new);

/// Produces the leader `BarrierWaitResult` from a single-participant barrier.
fn bwr_true() -> BarrierWaitResult {
  Barrier::new(1).wait()
}

/// Produces a `BarrierWaitResult` from a two-participant barrier.
///
/// Spawns a second thread so this thread's `wait` can return, then combines
/// the two results into the non-leader outcome. If the thread cannot be
/// spawned, it degrades to the single-participant leader result.
#[allow(
  clippy::single_call_fn,
  reason = "spawn a second thread to produce the two-participant BarrierWaitResult case"
)]
fn bwr_false() -> BarrierWaitResult {
  let barrier = Arc::new(Barrier::new(2));
  let b2 = Arc::clone(&barrier);
  // `thread::Builder::spawn` reports spawn failure as a `Result` where
  // `thread::spawn` would panic. The second participant must exist before
  // this thread may call `wait` (a lone `wait` on a 2-barrier blocks
  // forever), so on spawn failure degrade to the single-participant
  // (leader) result instead.
  thread::Builder::new().spawn(move || b2.wait()).map_or_else(
    |_| bwr_true(),
    |join_handle| {
      let bwr1 = barrier.wait();
      match join_handle.join() {
        Ok(bwr2) => {
          if bwr1.is_leader() {
            bwr2
          } else {
            bwr1
          }
        }
        // `join` only fails if the child panicked, and
        // `Barrier::wait` does not panic — keep this total by
        // degrading to the already-held result.
        Err(_) => bwr1,
      }
    },
  )
}

arbitrary!(RecvError; RecvError);

arbitrary!([T: Arbitrary] SendError<T>, SMapped<T, Self>, T::Parameters;
    args => static_map(any_with::<T>(args), SendError)
);

arbitrary!(RecvTimeoutError, TupleUnion<(WeightedStrategy<Just<Self>>, WeightedStrategy<Just<Self>>)>;
    prop_oneof![
        Just(RecvTimeoutError::Disconnected),
        Just(RecvTimeoutError::Timeout)
    ]
);

arbitrary!(TryRecvError, TupleUnion<(WeightedStrategy<Just<Self>>, WeightedStrategy<Just<Self>>)>;
    prop_oneof![
        Just(TryRecvError::Disconnected),
        Just(TryRecvError::Empty)
    ]
);

arbitrary!(
    [P: Clone + Default, T: Arbitrary<Parameters = P>] TrySendError<T>,
    TupleUnion<(WeightedStrategy<SMapped<T, Self>>, WeightedStrategy<SMapped<T, Self>>)>, P;
    args => prop_oneof![
        static_map(any_with::<T>(args.clone()), TrySendError::Disconnected),
        static_map(any_with::<T>(args), TrySendError::Full),
    ]
);

// If only half of a pair is generated then you will get a hang-up.
// Thus the only meaningful impls are in pairs.
arbitrary!([A] (Sender<A>, Receiver<A>), LazyJustFn<Self>;
    LazyJust::new(channel)
);

arbitrary!([A: fmt::Debug] (Sender<A>, IntoIter<A>), LazyJustFn<Self>;
    LazyJust::new(|| {
        let (rx, tx) = channel();
        (rx, tx.into_iter())
    })
);

arbitrary!([A] (SyncSender<A>, Receiver<A>), SMapped<u16, Self>;
    static_map(any::<u16>(), |size| sync_channel(usize::from(size)))
);

arbitrary!([A: fmt::Debug] (SyncSender<A>, IntoIter<A>), SMapped<u16, Self>;
    static_map(any::<u16>(), |size| {
        let (rx, tx) = sync_channel(usize::from(size));
        (rx, tx.into_iter())
    })
);

#[cfg(test)]
mod test {
  use super::*;

  no_panic_test!(
      barrier => Barrier,
      barrier_wait_result => BarrierWaitResult,
      once => Once,
      recv_error => RecvError,
      send_error => SendError<u8>,
      recv_timeout_error => RecvTimeoutError,
      try_recv_error => TryRecvError,
      try_send_error => TrySendError<u8>,
      rx_tx => (Sender<u8>, Receiver<u8>),
      rx_txiter => (Sender<u8>, IntoIter<u8>),
      syncrx_tx => (SyncSender<u8>, Receiver<u8>),
      syncrx_txiter => (SyncSender<u8>, IntoIter<u8>)
  );
}
