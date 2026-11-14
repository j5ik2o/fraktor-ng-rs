//! The view an actor gets while it handles one message.

use crate::kernel::actor::Actor;
use crate::kernel::actor::BehaviorStack;
use crate::kernel::actor::Props;
use crate::kernel::actor::Running;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::Executor;
use crate::kernel::message::Message;
use crate::kernel::supervision::SupervisorStrategy;

/// What an actor may do while it handles a message: address itself, name its
/// sender, send messages, start children, swap behaviour, and stop itself after
/// a reply.
///
/// Mirrors `org.apache.pekko.actor.ActorContext`. Pekko keeps the context on
/// the actor and lets it be read at any time; the port lends it out only for
/// the duration of one `receive` call, as the ownership model decides.
///
/// The context borrows the executor for the duration of the call. The executor
/// takes the running actor out of its table before it builds this context, so
/// the actor may start a child or stop itself through the context without
/// borrowing the actor table twice. The context also borrows the running
/// actor's behaviour stack, which `change_behavior`, `push_behavior`, and
/// `pop_behavior` update while a frame runs.
pub struct ActorContext<'a> {
  self_ref: ActorRef,
  sender: ActorRef,
  executor: &'a mut Executor,
  stack: &'a mut BehaviorStack,
}

impl<'a> ActorContext<'a> {
  pub(crate) fn new(
    self_ref: ActorRef,
    sender: ActorRef,
    executor: &'a mut Executor,
    stack: &'a mut BehaviorStack,
  ) -> Self {
    Self { self_ref, sender, executor, stack }
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

  /// Replaces the current behaviour with `behavior`; the frames below it stay.
  ///
  /// Mirrors `context.become(behavior, discardOld = true)`, which is
  /// `behavior :: behaviorStack.tail`: only the current head is dropped and the
  /// frames below it remain. `become` is a reserved word in Rust, so the method
  /// is named for what it does. The new behaviour answers the next message; a
  /// later `pop_behavior` reaches the frame that was below the replaced head.
  pub fn change_behavior<F>(&mut self, behavior: F)
  where
    F: FnMut(&mut ActorContext<'_>, Message) + 'static,
  {
    self.stack.change(Box::new(behavior));
  }

  /// Installs `behavior` above the current behaviour without dropping it.
  ///
  /// Mirrors `context.become(behavior, discardOld = false)`. A later
  /// `pop_behavior` restores the behaviour below.
  pub fn push_behavior<F>(&mut self, behavior: F)
  where
    F: FnMut(&mut ActorContext<'_>, Message) + 'static,
  {
    self.stack.push(Box::new(behavior));
  }

  /// Drops the top behaviour, restoring the one below it.
  ///
  /// Mirrors `context.unbecome()`. Called from the frame that is running, it
  /// drops that frame once it returns, so the next message reaches the
  /// behaviour below it (or the initial `receive`).
  pub fn pop_behavior(&mut self) {
    self.stack.pop();
  }

  /// Runs the behaviour of `initial` for one message.
  ///
  /// The top frame of the stack runs when there is one; otherwise the initial
  /// `Actor::receive` runs. The frame is taken out of the stack before it runs,
  /// so it may call `change_behavior`, `push_behavior`, or `pop_behavior` on
  /// the same stack through this context, and the stack it leaves behind is the
  /// one Pekko holds after the same calls.
  pub(crate) fn run_behavior(&mut self, initial: &mut dyn Actor, message: Message) {
    let mut running = self.stack.take_running();
    match &mut running {
      | Running::Frame { frame, .. } => frame(self, message),
      | Running::Receive => initial.receive(self, message),
    }
    self.stack.finish_running(running);
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
