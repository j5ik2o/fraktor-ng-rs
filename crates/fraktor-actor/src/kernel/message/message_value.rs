//! A type-erased message sent to an actor.

use alloc::sync::Arc;
use core::any::Any;
use core::any::TypeId;
use core::fmt;

#[cfg(test)]
#[path = "message_value_test.rs"]
mod tests;

/// A message that carries a value of any type that can cross a thread.
///
/// Mirrors the object an `org.apache.pekko.actor.Actor` receives in its
/// `receive` partial function. Pekko messages are untyped `AnyRef`; the port
/// keeps the same type erasure so that a mailbox can hold messages of
/// different types, while requiring `Send + Sync` because a message may cross
/// the boundary of its shard and because a test probe hands the same message
/// both to an auto pilot and to its queue.
///
/// A message is cheap to clone: cloning shares the payload instead of copying
/// it, which is how one delivery reaches more than one consumer.
#[derive(Clone)]
pub struct Message {
  payload: Arc<dyn Any + Send + Sync>,
}

impl Message {
  /// Wraps a value as a message.
  ///
  /// A `Message` given to this function is returned unchanged, so forwarding a
  /// message keeps the payload a recipient can compare against.
  pub fn new<M: Any + Send + Sync>(payload: M) -> Self {
    let boxed: Box<dyn Any + Send + Sync> = Box::new(payload);
    match boxed.downcast::<Message>() {
      | Ok(inner) => *inner,
      | Err(other) => Self { payload: Arc::from(other) },
    }
  }

  /// Returns the payload as the given type, or `None` if it has another type.
  pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
    self.payload.downcast_ref::<T>()
  }

  /// Returns the type of the payload it carries.
  ///
  /// A publisher that receives a `Message` needs this to route the message by
  /// the class of what it carries rather than the class of `Message` itself,
  /// which is how `EventStream` behaves in Pekko: an event is published to the
  /// subscribers of `event.getClass`.
  pub(crate) fn payload_type_id(&self) -> TypeId {
    (*self.payload).type_id()
  }

  /// Returns whether the payload has the given type.
  pub fn is<T: Any>(&self) -> bool {
    self.payload.is::<T>()
  }
}

impl fmt::Debug for Message {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.payload.downcast_ref::<&'static str>() {
      | Some(text) => write!(formatter, "{text:?}"),
      | None => match self.payload.downcast_ref::<i32>() {
        | Some(number) => write!(formatter, "{number}"),
        | None => formatter.write_str("<message of an unknown type>"),
      },
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

impl PartialEq<&str> for Message {
  fn eq(&self, other: &&str) -> bool {
    self == *other
  }
}

impl PartialEq<str> for &Message {
  fn eq(&self, other: &str) -> bool {
    *self == other
  }
}

impl PartialEq<Message> for &str {
  fn eq(&self, other: &Message) -> bool {
    other == *self
  }
}
