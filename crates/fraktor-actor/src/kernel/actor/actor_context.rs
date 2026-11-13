//! The view an actor gets while it handles one message.

use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::Executor;
use crate::kernel::message::Message;
use crate::kernel::supervision::SupervisorStrategy;

/// What an actor may do while it handles a message: address itself, name its
/// sender, send messages, start children, and stop itself after a reply.
///
/// Mirrors `org.apache.pekko.actor.ActorContext`. Pekko keeps the context on
/// the actor and lets it be read at any time; the port lends it out only for
/// the duration of one `receive` call, as the ownership model decides.
///
/// The context borrows the executor for the duration of the call. The executor
/// takes the running actor out of its table before it builds this context, so
/// the actor may start a child or stop itself through the context without
/// borrowing the actor table twice.
pub struct ActorContext<'a> {
  self_ref: ActorRef,
  sender: ActorRef,
  executor: &'a mut Executor,
}

impl<'a> ActorContext<'a> {
  pub(crate) fn new(self_ref: ActorRef, sender: ActorRef, executor: &'a mut Executor) -> Self {
    Self { self_ref, sender, executor }
  }

  /// Returns a handle to the actor that is handling the message.
  pub fn self_ref(&self) -> ActorRef {
    self.self_ref.clone()
  }

  /// Returns a handle to the actor that sent the message being handled.
  pub fn sender(&self) -> ActorRef {
    self.sender.clone()
  }

  /// Sends a message to `target`, naming this actor as the sender.
  pub fn tell(&mut self, target: &ActorRef, message: Message) {
    self.executor.send(self.self_ref(), target, message);
  }

  /// Starts a child of this actor with `props` and `strategy` and returns its
  /// handle.
  ///
  /// Mirrors `ActorContext.actorOf(props)`: Pekko's `actorOf` uses the parent's
  /// strategy, and this port lets the caller pass one so a test helper can put
  /// a child under a chosen strategy.
  pub fn actor_of_with_strategy(&mut self, props: Props, strategy: SupervisorStrategy) -> ActorRef {
    let parent = self.self_ref();
    self.executor.spawn_child(&parent, props, None, strategy)
  }

  /// Stops this actor and then sends `message` to `reply_to`.
  ///
  /// Mirrors a `context.stop(self)` followed by a reply piped from a background
  /// computation: the actor finishes the step it is on, its watchers learn that
  /// it is gone, and only then does `reply_to` receive the message.
  pub fn stop_and_reply_after_termination<M: core::any::Any + Send + Sync>(&mut self, reply_to: ActorRef, message: M) {
    let self_ref = self.self_ref();
    self.executor.stop_and_reply(&self_ref, reply_to, Message::new(message));
  }

  pub(crate) fn forward(&mut self, target: &ActorRef, message: Message) {
    let sender = self.sender();
    self.executor.send(sender, target, message);
  }
}
