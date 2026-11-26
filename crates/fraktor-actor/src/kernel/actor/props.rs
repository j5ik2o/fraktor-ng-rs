//! A recipe that creates one actor.

use alloc::boxed::Box;
use alloc::sync::Arc;

use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::message::Message;

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
  dispatcher: Option<Arc<str>>,
}

/// The actor that [`Props::empty`] builds.
///
/// Mirrors `Props.EmptyActor`, whose `receive` is `Actor.emptyBehavior`: it
/// covers no message. The port has no partial function to fall through, so the
/// actor reports each message it is given through
/// [`ActorContext::unhandled`](crate::kernel::actor::ActorContext::unhandled).
struct EmptyActor;

impl Actor for EmptyActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    context.unhandled(message);
  }
}

impl Props {
  /// Creates a recipe for an actor that handles no message.
  ///
  /// Mirrors `Props.empty`, which is `Props[EmptyActor]()` over
  /// `Actor.emptyBehavior`. Every message that reaches the actor is reported
  /// through [`ActorContext::unhandled`], so an actor built from this recipe
  /// answers nothing and reports each message it is sent.
  ///
  /// Do not make this actor a subscriber of
  /// [`UnhandledMessage`](crate::kernel::actor::UnhandledMessage) or of every
  /// event: it treats the report it receives as another message it does not
  /// handle and reports that in turn, so the stream never goes idle. Pekko
  /// behaves the same way, because `aroundReceive` calls `unhandled` for a
  /// message `emptyBehavior` does not match. A subscriber that watches
  /// unhandled messages must handle or ignore them.
  pub fn empty() -> Self {
    Self::new(|| EmptyActor)
  }

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
    Self { factory: Arc::new(move || Box::new(factory())), dispatcher: None }
  }

  /// Records the name of the dispatcher the actor should run on.
  ///
  /// Mirrors `Props.withDispatcher(name)`. The goal that introduces the child
  /// name records states that the port keeps the name only: interpreting a
  /// configuration text and modelling a thread pool belong to a later goal.
  /// The name therefore travels with the recipe and is readable through
  /// [`Props::dispatcher`] without changing how the actor runs.
  pub fn with_dispatcher(mut self, name: &str) -> Self {
    self.dispatcher = Some(Arc::from(name));
    self
  }

  /// Returns the dispatcher name the recipe records, when one was set.
  ///
  /// Mirrors `Props.dispatcher`, narrowed to the name record this port keeps.
  pub fn dispatcher(&self) -> Option<&str> {
    self.dispatcher.as_deref()
  }

  pub(crate) fn create_actor(&self) -> Box<dyn Actor> {
    (self.factory)()
  }
}

impl Clone for Props {
  fn clone(&self) -> Self {
    Self { factory: Arc::clone(&self.factory), dispatcher: self.dispatcher.clone() }
  }
}
