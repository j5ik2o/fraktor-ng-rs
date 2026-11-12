//! A recipe that creates one actor.

use alloc::boxed::Box;

use crate::kernel::actor::Actor;

/// A recipe that creates an actor when the system is asked to start it.
///
/// Mirrors `org.apache.pekko.actor.Props`, which packages a factory, the actor
/// class, and its arguments. The port keeps only the factory, because the
/// executor of this milestone creates one actor per recipe and needs nothing
/// else.
pub struct Props {
  factory: Box<dyn Fn() -> Box<dyn Actor>>,
}

impl Props {
  /// Creates a recipe from a factory.
  pub fn new<F, A>(factory: F) -> Self
  where
    F: Fn() -> A + 'static,
    A: Actor + 'static,
  {
    Self { factory: Box::new(move || Box::new(factory())) }
  }

  pub(crate) fn create_actor(&self) -> Box<dyn Actor> {
    (self.factory)()
  }
}
