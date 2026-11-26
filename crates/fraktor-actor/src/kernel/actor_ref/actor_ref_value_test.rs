use super::*;

use alloc::sync::Arc;

use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_path::RootActorPath;
use crate::kernel::address::Address;

struct SilentBoundary;

impl ShardBoundary for SilentBoundary {
  fn post(&self, _envelope: Envelope) {}

  fn poll(&self) -> Option<Envelope> {
    None
  }

  fn poll_within(&self, _max: core::time::Duration) -> Option<Envelope> {
    None
  }

  fn has_pending(&self) -> bool {
    false
  }

  fn close(&self) {}
}

fn reference(id: u64, generation: u32, name: &str) -> ActorRef {
  let root = RootActorPath::new(Address::new("pekko", "RefSpec"));
  let path = ActorPath::from_root(root).with_child(name);
  ActorRef::new(id, generation, Arc::new(path), Arc::new(SilentBoundary))
}

#[test]
fn a_clone_still_shares_the_delivery_handle() {
  let original = reference(7, 0, "child");
  let cloned = original.clone();
  assert!(original.shares_endpoint_with(&cloned), "a clone posts through the same handle");
  assert!(cloned.shares_endpoint_with(&original), "a clone posts through the same handle both ways");
  assert_eq!(cloned.path(), original.path(), "a clone carries the same path");
  assert!(cloned == original, "a clone carries the same identity");
}

#[test]
fn references_with_different_identities_never_compare_equal() {
  let first = reference(1, 0, "a");
  let second = reference(2, 0, "b");
  assert_ne!(first, second, "references with different identifiers must not be equal even with the same generation");
}

#[test]
fn equal_references_of_different_paths_still_compare_equal() {
  // Equality uses the identifier and generation only; the path is carried but
  // never decides a match. A colliding reference therefore has to keep the
  // old path and boundary behaviour on top of that identity, not replace it.
  let first = reference(1, 0, "a");
  let second = reference(1, 0, "b");
  assert!(first == second, "different paths do not change the identity");
  assert_ne!(first.path(), second.path(), "the paths still differ even though the identities match");
}

#[test]
fn a_detached_noop_keeps_the_identity_and_path_over_a_separate_delivery() {
  // The colliding reference the UidClash case watches must collide with the old
  // child (same identifier and generation) and carry its full path, while
  // posting into a delivery entity of its own.
  let original = reference(9, 3, "child");
  let detached = original.detached_noop();
  assert!(detached == original, "the detached reference must keep the identity it was made from");
  assert_eq!(detached.path(), original.path(), "the detached reference must carry the original path");
  assert!(
    !detached.shares_endpoint_with(&original),
    "the detached reference must post into a separate delivery entity"
  );
  assert!(detached.is_detached(), "the detached reference must be tagged as no-op for the watch boundary");
  assert!(!original.is_detached(), "the original handle must not be tagged as no-op");
  assert!(!original.clone().is_detached(), "a clone of the original must not become no-op");
}

// What the tests of stage 3 read of a handle's mailbox (#252).
impl ActorRef {
  /// Returns a weak handle to the actor's mailbox, for a test that the
  /// mailbox goes away.
  pub(crate) fn mailbox_weak(&self) -> crate::kernel::sync::Weak<Mailbox<ActorCell>> {
    match &self.endpoint {
      | Endpoint::Mailbox(mailbox) => Arc::downgrade(mailbox),
      | Endpoint::Boundary(_) => panic!("{self:?} has no mailbox"),
    }
  }

  /// Returns how many strong handles the actor's mailbox has, for a test
  /// that no clone leaks.
  pub(crate) fn mailbox_strong_count(&self) -> usize {
    match &self.endpoint {
      | Endpoint::Mailbox(mailbox) => Arc::strong_count(mailbox),
      | Endpoint::Boundary(_) => panic!("{self:?} has no mailbox"),
    }
  }
}
