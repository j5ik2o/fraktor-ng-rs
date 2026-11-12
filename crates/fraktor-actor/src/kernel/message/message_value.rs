//! A type-erased message sent to an actor.

use core::any::Any;
use core::fmt;

#[cfg(test)]
#[path = "message_value_test.rs"]
mod tests;

/// A message that carries a value of any type that can cross a thread.
///
/// Mirrors the object an `org.apache.pekko.actor.Actor` receives in its
/// `receive` partial function. Pekko messages are untyped `AnyRef`; the port
/// keeps the same type erasure so that a mailbox can hold messages of
/// different types, while requiring `Send` because a message may cross the
/// boundary of its shard.
pub struct Message {
  payload: Box<dyn Any + Send>,
}

impl Message {
  /// Wraps a value as a message.
  pub fn new<M: Any + Send>(payload: M) -> Self {
    Self { payload: Box::new(payload) }
  }

  /// Returns the payload as the given type, or `None` if it has another type.
  pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
    self.payload.downcast_ref::<T>()
  }
}

impl fmt::Debug for Message {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.payload.downcast_ref::<&'static str>() {
      | Some(text) => write!(formatter, "{text:?}"),
      | None => formatter.write_str("<message of an unknown type>"),
    }
  }
}

impl PartialEq<str> for Message {
  fn eq(&self, other: &str) -> bool {
    match self.payload.downcast_ref::<&'static str>() {
      | Some(text) => *text == other,
      | None => false,
    }
  }
}
