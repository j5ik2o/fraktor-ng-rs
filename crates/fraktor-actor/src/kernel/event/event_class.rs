//! The run-time class of an event, used as an event stream channel.

use core::any::TypeId;
use core::fmt;
use core::hash::{Hash, Hasher};

/// The run-time class of an event.
///
/// Mirrors the `Class[_]` that `org.apache.pekko.event.EventStream` uses to
/// name a channel. Rust has no class hierarchy at run time, so the port names a
/// class by its `TypeId` and lets the stream record the subclass links that a
/// test declares. [`EventClass::any`] is the class every other class is a
/// subclass of, which is Pekko's `classOf[Any]`. The name is kept for
/// diagnostics only; it takes no part in equality.
#[derive(Clone, Copy)]
pub struct EventClass {
  type_id: Option<TypeId>,
  name: &'static str,
}

impl EventClass {
  /// Returns the class of `T`.
  pub fn of<T: 'static>() -> Self {
    Self { type_id: Some(TypeId::of::<T>()), name: core::any::type_name::<T>() }
  }

  /// Returns the class every other class is a subclass of.
  ///
  /// Mirrors `classOf[Any]`, the channel a subscriber listens to when it wants
  /// every event of the stream.
  pub fn any() -> Self {
    Self { type_id: None, name: "core::any::Any" }
  }
}

impl PartialEq for EventClass {
  fn eq(&self, other: &Self) -> bool {
    self.type_id == other.type_id
  }
}

impl Eq for EventClass {}

impl Hash for EventClass {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.type_id.hash(state);
  }
}

impl fmt::Display for EventClass {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "EventClass({})", self.name)
  }
}

impl fmt::Debug for EventClass {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "EventClass({})", self.name)
  }
}
