//! Unit tests for [`ChannelBoundary`](super::ChannelBoundary).

use core::time::Duration;
use std::sync::Arc;

use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::Props;
use crate::kernel::actor_system::ActorSystem;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Message;

use super::ChannelBoundary;

struct Sink;

impl Actor for Sink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

fn system_on(boundary: &Arc<ChannelBoundary>) -> ActorSystem {
  ActorSystem::new("ChannelBoundarySpec", Arc::clone(boundary) as Arc<_>)
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
  let mut system = system_on(&boundary);
  let recipient = system.actor_of(Props::new(|| Sink));

  recipient.tell(Message::new("hello"), recipient.clone());

  assert!(boundary.poll().is_some());
}

#[test]
fn polling_within_finds_a_message_that_tell_posted() {
  let boundary = Arc::new(ChannelBoundary::new());
  let mut system = system_on(&boundary);
  let recipient = system.actor_of(Props::new(|| Sink));

  recipient.tell(Message::new("hello"), recipient.clone());

  assert!(boundary.poll_within(Duration::from_millis(10)).is_some());
}

#[test]
fn posting_after_close_is_discarded() {
  let boundary = Arc::new(ChannelBoundary::new());
  let mut system = system_on(&boundary);
  let recipient = system.actor_of(Props::new(|| Sink));

  boundary.close();
  recipient.tell(Message::new("hello"), recipient.clone());

  assert!(boundary.poll().is_none());
}
