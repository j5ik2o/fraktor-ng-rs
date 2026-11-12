//! Common actor patterns that tests use.

mod echo_actor;
mod forward_actor;
mod test_actors_props;

pub use echo_actor::EchoActor;
pub use forward_actor::ForwardActor;
pub use test_actors_props::echo_actor_props;
pub use test_actors_props::forward_actor_props;
