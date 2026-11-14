//! Unit tests for [`Terminated`](super::Terminated).

use alloc::sync::Arc;

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorId;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::actor_ref::Generation;

use super::Terminated;

fn actor_ref(id: u64) -> ActorRef {
  let path = ActorPath::from_string("pekko://test/user/target").expect("a valid path");
  ActorRef::new(ActorId::new(id), Generation::new(0), Arc::new(path), Arc::new(ChannelBoundary::new()))
}

#[test]
fn notices_of_the_same_target_are_equal_whatever_the_flags() {
  let target = actor_ref(1);
  assert_eq!(Terminated::new(target.clone(), false, false), Terminated::new(target, true, true));
}

#[test]
fn notices_of_different_targets_are_not_equal() {
  assert_ne!(Terminated::new(actor_ref(1), false, false), Terminated::new(actor_ref(2), false, false));
}
