//! A recipe that creates one actor.

use alloc::boxed::Box;
use alloc::sync::Arc;

use crate::kernel::actor::Actor;

#[cfg(test)]
#[path = "props_test.rs"]
mod tests;

/// A recipe that creates an actor when the system is asked to start it.
///
/// Mirrors `org.apache.pekko.actor.Props`, which packages a factory, the actor
/// class, and its arguments. The port keeps only the factory, because the
/// executor of this milestone creates one actor per recipe and needs nothing
/// else. A recipe is reusable: a supervisor restarts a failed child by
/// building a fresh actor from the same recipe.
///
/// A recipe travels as a message, because a test supervisor answers an `ask`
/// with the recipe for the child it creates. A `Message` must be `Send + Sync`,
/// so the recipe must be too.
pub struct Props {
  factory: Arc<dyn Fn() -> Box<dyn Actor> + Send + Sync>,
}

impl Props {
  /// Creates a recipe from a factory.
  ///
  /// The factory is wrapped so that a recipe can travel as a message and be
  /// cloned, which a shared `Arc` gives for free. A recipe inherits the
  /// `Send + Sync` bound of `Message`, so the factory must satisfy it: a
  /// recipe that crosses a shard boundary is created on the receiving thread.
  pub fn new<F, A>(factory: F) -> Self
  where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor + 'static,
  {
    Self { factory: Arc::new(move || Box::new(factory())) }
  }

  pub(crate) fn create_actor(&self) -> Box<dyn Actor> {
    (self.factory)()
  }
}

impl Clone for Props {
  fn clone(&self) -> Self {
    Self { factory: Arc::clone(&self.factory) }
  }
}
