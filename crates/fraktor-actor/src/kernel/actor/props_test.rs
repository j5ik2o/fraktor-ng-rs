//! Unit tests for [`Props`](super::Props).

use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::Props;
use crate::kernel::message::Message;

/// A behaviour that only has to exist, so a recipe can create it.
struct NoOp;

impl Actor for NoOp {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn a_recipe_is_send_and_sync_so_it_can_travel_as_a_message() {
  assert_send_sync::<Props>();
}

#[test]
fn a_recipe_can_be_carried_inside_a_message() {
  let props = Props::new(|| NoOp);

  let message = Message::new(props);

  assert!(message.is::<Props>());
}
