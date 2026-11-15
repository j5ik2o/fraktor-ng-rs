//! The error a parent raises when a child name is already taken.

use alloc::format;
use alloc::string::String;
use core::any::Any;
use core::fmt;

/// The error a parent raises when it is asked for a child whose name a live
/// child of the same parent already uses.
///
/// Mirrors `org.apache.pekko.actor.InvalidActorNameException`. Pekko throws it
/// from `Children.attachChild` when a name is taken or the parent is
/// terminating; the port returns it from `ActorContext::actor_of` so a caller
/// can react to it without unwinding. The name may be reused once the child
/// that held it has stopped.
#[derive(Clone, PartialEq, Eq)]
pub struct InvalidActorNameException {
  cause: String,
}

impl InvalidActorNameException {
  /// Creates the error for `name` being taken by a live child of the parent.
  pub fn name_taken(name: &str) -> Self {
    Self { cause: format!("Actor name [{}] is not unique!", name) }
  }

  /// Creates the error for a parent that is terminating or terminated.
  ///
  /// Mirrors the `IllegalStateException` that `Children.attachChild` throws
  /// when a defunct guardian is asked for a child.
  pub fn terminating() -> Self {
    Self { cause: String::from("cannot create children while terminating or terminated") }
  }

  /// Returns whether this error is an instance of `T`.
  ///
  /// Mirrors Pekko's `e match { case _: T => ... }`: a caller that holds a
  /// concrete error narrows it the same way it would narrow a caught
  /// exception. The port keeps the shape on the error so a caller reads a
  /// single value instead of matching a wrapper.
  pub fn is<T: Any>(&self) -> bool {
    core::any::TypeId::of::<T>() == core::any::TypeId::of::<Self>()
  }
}

impl fmt::Display for InvalidActorNameException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.cause)
  }
}

impl fmt::Debug for InvalidActorNameException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.debug_tuple("InvalidActorNameException").field(&self.cause).finish()
  }
}

impl core::error::Error for InvalidActorNameException {}
