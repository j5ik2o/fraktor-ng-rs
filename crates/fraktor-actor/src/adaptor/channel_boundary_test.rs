//! Unit tests for [`ChannelBoundary`](super::ChannelBoundary).

use core::time::Duration;
use std::sync::Arc;
use std::sync::Barrier;
use std::thread;

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::actor_system::ActorSystem;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Message;

use super::ChannelBoundary;

fn system_on(boundary: &Arc<ChannelBoundary>) -> ActorSystem {
  ActorSystem::new("ChannelBoundarySpec", Arc::clone(boundary) as Arc<_>)
}

// A handle without a mailbox: a path that no actor has, which resolves as
// Pekko's `EmptyLocalActorRef` does. Its messages go through the shard
// boundary, so a test of the boundary posts with it. A tell to an actor goes
// onto the actor's mailbox and never reaches the boundary.
fn without_mailbox(system: &ActorSystem) -> ActorRef {
  let root = system.dead_letters().path().parent();
  system.resolve_actor_ref(&root.with_child("user").with_child("no-actor-here"))
}

#[test]
fn polling_an_idle_boundary_returns_none() {
  let boundary = ChannelBoundary::new();

  assert!(boundary.poll().is_none());
}

#[test]
fn polling_within_returns_none_when_nothing_arrives() {
  let boundary = ChannelBoundary::new();

  assert!(boundary.poll_within(Duration::from_millis(10)).is_none());
}

#[test]
fn polling_finds_a_message_that_tell_posted() {
  let boundary = Arc::new(ChannelBoundary::new());
  let system = system_on(&boundary);
  let recipient = without_mailbox(&system);

  recipient.tell(Message::new("hello"), recipient.clone());

  assert!(boundary.poll().is_some());
}

#[test]
fn polling_within_finds_a_message_that_tell_posted() {
  let boundary = Arc::new(ChannelBoundary::new());
  let system = system_on(&boundary);
  let recipient = without_mailbox(&system);

  recipient.tell(Message::new("hello"), recipient.clone());

  assert!(boundary.poll_within(Duration::from_millis(10)).is_some());
}

#[test]
fn posting_after_close_is_discarded() {
  let boundary = Arc::new(ChannelBoundary::new());
  let system = system_on(&boundary);
  let recipient = without_mailbox(&system);

  boundary.close();
  recipient.tell(Message::new("hello"), recipient.clone());

  assert!(boundary.poll().is_none());
}

#[test]
fn a_post_racing_close_leaves_nothing_after_the_drain() {
  const POSTERS: usize = 4;
  const POSTS_PER_POSTER: usize = 200;
  const ROUNDS: usize = 200;

  for _ in 0..ROUNDS {
    let boundary = Arc::new(ChannelBoundary::new());
    let system = system_on(&boundary);
    let recipient = without_mailbox(&system);
    let barrier = Arc::new(Barrier::new(POSTERS + 1));

    let mut posters = Vec::with_capacity(POSTERS);
    for _ in 0..POSTERS {
      let recipient = recipient.clone();
      let barrier = Arc::clone(&barrier);
      posters.push(thread::spawn(move || {
        barrier.wait();
        for _ in 0..POSTS_PER_POSTER {
          recipient.tell(Message::new("racing"), recipient.clone());
        }
      }));
    }

    barrier.wait();
    boundary.close();
    while boundary.poll().is_some() {}

    for poster in posters {
      poster.join().unwrap();
    }

    assert!(boundary.poll().is_none());
  }
}

#[test]
fn has_pending_reports_a_posted_envelope_until_it_is_taken() {
  let boundary = Arc::new(ChannelBoundary::new());
  let system = system_on(&boundary);
  let recipient = without_mailbox(&system);

  assert!(!boundary.has_pending());
  recipient.tell(Message::new("hello"), recipient.clone());
  assert!(boundary.has_pending());

  assert!(boundary.poll().is_some());
  assert!(!boundary.has_pending());
}

#[test]
fn has_pending_ignores_a_post_that_close_rejected() {
  let boundary = Arc::new(ChannelBoundary::new());
  let system = system_on(&boundary);
  let recipient = without_mailbox(&system);

  boundary.close();
  recipient.tell(Message::new("hello"), recipient.clone());

  assert!(!boundary.has_pending());
}
