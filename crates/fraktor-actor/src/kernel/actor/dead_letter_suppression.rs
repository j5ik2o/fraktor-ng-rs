//! The marker for messages whose dead letter report stays out of the log.

/// Marks a message type whose dead letters are reported as
/// [`SuppressedDeadLetter`](super::SuppressedDeadLetter) instead of
/// [`DeadLetter`](super::DeadLetter).
///
/// Mirrors `org.apache.pekko.actor.DeadLetterSuppression`. Pekko tests the
/// marker with `isInstanceOf` at run time; Rust cannot test trait
/// membership at run time, so a type that implements this trait must also
/// be registered with
/// [`ActorSystem::register_dead_letter_suppression`](crate::kernel::actor_system::ActorSystem::register_dead_letter_suppression),
/// which records its `TypeId` in the table the routing rules read. The
/// trait itself carries no behaviour; it exists so the intent is visible at
/// the type, as in Pekko.
pub trait DeadLetterSuppression {}
