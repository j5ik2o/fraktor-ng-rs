//! The registry that lets a waiting `ask` drive the shard that owes its reply.
//!
//! A ported test asks an actor on the test thread and waits for the reply with
//! `await_result`. The port drives a shard from a single thread, so a wait that
//! only blocked would never let the asked actor run. Pekko has the same
//! situation from the test's point of view: the dispatched actor works in the
//! background while the test blocks. The port keeps the single-threaded driver
//! and lets the wait do the driving instead.
//!
//! A spec registers the driver of its shard, keyed by the address of the shard's
//! boundary, which an `ActorRef` carries. An `ask` keeps the key of the
//! recipient, and `await_result` steps that driver until the reply arrives or
//! the deadline passes. A recipient whose shard was never registered, such as a
//! function reference answered synchronously, is not driven and keeps waiting
//! on its condition variable.

use core::cell::RefCell;
use core::time::Duration;

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::kernel::actor_system::WeakActorSystem;

/// Steps one shard by at most one message. Returns whether it delivered one.
pub(crate) type Step = Box<dyn FnMut() -> bool>;

thread_local! {
  /// The drivers of the shards this thread owns, keyed by boundary address.
  static DRIVERS: RefCell<Vec<(usize, Step)>> = const { RefCell::new(Vec::new()) };
}

/// Registers a wait driver for `system` on the current thread.
///
/// A ported test asks an actor and waits for the reply on the test thread. The
/// port drives a shard from one thread, so the wait has to run the system; this
/// registers the step that a waiting `ask` uses to do so. Registering again for
/// the same shard replaces the earlier step.
pub fn attach_wait_driver(system: &crate::kernel::actor_system::ActorSystem) {
  let key = system.shard_key();
  let handle = system.downgrade();
  register(key, Box::new(move || step_system(&handle)));
}

fn step_system(system: &WeakActorSystem) -> bool {
  system.run_one()
}

/// Registers `step` for the shard whose boundary has address `key`.
///
/// A later registration for the same key replaces the earlier one, so a spec
/// that re-registers after a restart of its system drives the current shard.
pub(crate) fn register(key: usize, step: Step) {
  DRIVERS.with(|drivers| {
    let mut drivers = drivers.borrow_mut();
    drivers.retain(|(existing, _)| *existing != key);
    drivers.push((key, step));
  });
}

/// Steps the driver of `key` once, returning whether it delivered a message.
pub(crate) fn step(key: usize) -> bool {
  DRIVERS.with(|drivers| {
    let mut drivers = drivers.borrow_mut();
    match drivers.iter_mut().find(|(existing, _)| *existing == key) {
      | Some((_, step)) => step(),
      | None => false,
    }
  })
}

/// Steps the driver of `key` until `completed` returns true or `max` passes.
///
/// Returns whether the wait completed. A key that was never registered is not
/// stepped, so the caller falls back to its own waiting.
pub(crate) fn drive_until(key: usize, max: Duration, mut completed: impl FnMut() -> bool) -> bool {
  if !is_registered(key) {
    return false;
  }
  let deadline = std::time::Instant::now() + max;
  loop {
    if completed() {
      return true;
    }
    if std::time::Instant::now() >= deadline {
      return completed();
    }
    if !step(key) {
      std::thread::sleep(Duration::from_millis(1));
    }
  }
}

/// Returns whether a driver was registered for `key`.
pub(crate) fn is_registered(key: usize) -> bool {
  DRIVERS.with(|drivers| drivers.borrow().iter().any(|(existing, _)| *existing == key))
}
