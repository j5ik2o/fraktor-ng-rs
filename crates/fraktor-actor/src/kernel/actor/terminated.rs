//! The message a watcher receives when the actor it watches stops.

use core::fmt;

use crate::kernel::actor_ref::ActorRef;

#[cfg(test)]
#[path = "terminated_test.rs"]
mod tests;

/// The notice that a watched actor has stopped.
///
/// Mirrors `org.apache.pekko.actor.Terminated`. Pekko carries three facts:
/// which actor stopped, whether its existence was confirmed before the watch,
/// and whether a remote address terminated. The port keeps the same three.
/// Equality uses only the target, as Pekko's first parameter list does, so a
/// test can compare against `Terminated(target)` without knowing the flags.
#[derive(Clone)]
pub struct Terminated {
  target: ActorRef,
  existence_confirmed: bool,
  address_terminated: bool,
}

impl Terminated {
  pub(crate) fn new(target: ActorRef, existence_confirmed: bool, address_terminated: bool) -> Self {
    Self { target, existence_confirmed, address_terminated }
  }

  /// Returns the actor that stopped.
  pub fn target(&self) -> &ActorRef {
    &self.target
  }

  /// Returns whether the watch confirmed that the actor existed.
  pub fn existence_confirmed(&self) -> bool {
    self.existence_confirmed
  }

  /// Returns whether a remote address terminated rather than the actor.
  pub fn address_terminated(&self) -> bool {
    self.address_terminated
  }
}

impl PartialEq for Terminated {
  fn eq(&self, other: &Self) -> bool {
    self.target == other.target
  }
}

impl Eq for Terminated {}

impl fmt::Debug for Terminated {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      formatter,
      "Terminated({:?}, existence_confirmed: {}, address_terminated: {})",
      self.target, self.existence_confirmed, self.address_terminated
    )
  }
}
