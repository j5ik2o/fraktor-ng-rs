//! The recipes that start the test actors.

use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_ref::ActorRef;

use crate::test_actors::EchoActor;
use crate::test_actors::ForwardActor;

/// Returns the recipe of an [`EchoActor`], corresponding to
/// `TestActors.echoActorProps`.
pub fn echo_actor_props() -> Props {
  EchoActor::props()
}

/// Returns the recipe of a [`ForwardActor`] towards `target`, corresponding to
/// `TestActors.forwardActorProps(ref)`.
pub fn forward_actor_props(target: ActorRef) -> Props {
  ForwardActor::props(target)
}
