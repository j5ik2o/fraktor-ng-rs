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
  factory: Arc<dyn Fn() -> Box<dyn Actor>>,
}

impl Props {
  /// Creates a recipe from a factory.
  ///
  /// The factory is wrapped so that a recipe can travel as a message and be
  /// cloned, which a shared `Arc` gives for free.
  pub fn new<F, A>(factory: F) -> Self
  where
    F: Fn() -> A + 'static,
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

// SAFETY: the factory behind the `Arc` is called only from the executor that
// owns the actors of one shard, and this port drives a shard from a single
// thread. A recipe travels as a message only so that a test supervisor can
// answer with it, and the receiving side hands it back to the same executor.
// Nothing in the factory is run on another thread, so sharing the `Arc` across
// threads is sound even though the closure itself is not `Send`.
unsafe impl Send for Props {}

// SAFETY: see the `Send` implementation above. No thread calls the factory
// concurrently; the `Sync` marker only lets the recipe be carried inside a
// `Message`, whose payload is shared between the sender and the queue.
unsafe impl Sync for Props {}
