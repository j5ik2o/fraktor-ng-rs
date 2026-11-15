//! Actors, their context, and the recipes that create them.

// Public path fixed by the Pekko port (org.apache.pekko.event.Logging).
#[allow(non_snake_case)]
pub mod Logging;

mod actor_context;
mod actor_trait;
mod behavior_stack;
mod invalid_actor_name_exception;
mod props;
mod terminated;

pub use actor_context::ActorContext;
pub use actor_trait::Actor;
pub(crate) use behavior_stack::BehaviorStack;
pub(crate) use behavior_stack::Running;
pub use invalid_actor_name_exception::InvalidActorNameException;
pub use props::Props;
pub use terminated::Terminated;
