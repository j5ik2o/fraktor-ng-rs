//! Unit tests for the crate-private pieces of the switch (outside the
//! original SwitchSpec): the revert guard reverts on drop and keeps the
//! change once defused.

use core::sync::atomic::{AtomicBool, Ordering};

use super::super::revert_guard::RevertGuard;

#[test]
fn revert_guard_reverts_on_drop() {
  let on = AtomicBool::new(true);
  {
    let _guard = RevertGuard::new(&on, false);
  }
  assert!(!on.load(Ordering::SeqCst), "the guard reverted the flipped state back to `from`");
}

#[test]
fn revert_guard_keeps_the_change_once_defused() {
  let on = AtomicBool::new(true);
  {
    let mut guard = RevertGuard::new(&on, false);
    guard.defuse();
  }
  assert!(on.load(Ordering::SeqCst), "a defused guard leaves the state alone");
}
