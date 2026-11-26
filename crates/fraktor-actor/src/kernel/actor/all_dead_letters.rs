//! The channel that covers every dead letter event.

/// The channel that carries every dead letter event.
///
/// Mirrors `org.apache.pekko.actor.AllDeadLetters`, the trait that
/// `DeadLetter`, `SuppressedDeadLetter`, `Dropped`, and `UnhandledMessage`
/// all extend: a subscription to this channel receives each of them,
/// because the stream declares each of those classes a subclass of this
/// one when the system is created. The type itself is never constructed as
/// a value; it only names the channel, so it cannot clash with a message
/// type of the same name.
pub struct AllDeadLetters;
