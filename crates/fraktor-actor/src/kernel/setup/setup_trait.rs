//! A part of the setup an actor system is created with.

use core::any::Any;

use crate::kernel::setup::ActorSystemSetup;

/// A setup part that can be put inside an [`ActorSystemSetup`].
///
/// Mirrors `org.apache.pekko.actor.setup.Setup`. The concrete type of a setup
/// is its key: an [`ActorSystemSetup`] holds at most one value of each type.
/// No setup is mandatory, so a system created without one uses its defaults.
pub trait Setup: Any + Send + Sync {
  /// Returns a setup holding this setup and `other`, which replaces this one
  /// when both have the same concrete type.
  ///
  /// Mirrors `Setup.and`, which allows `a and b and c` together with
  /// [`ActorSystemSetup::and`].
  fn and<S: Setup>(self, other: S) -> ActorSystemSetup
  where
    Self: Sized,
  {
    ActorSystemSetup::empty().with_setup(self).with_setup(other)
  }
}
