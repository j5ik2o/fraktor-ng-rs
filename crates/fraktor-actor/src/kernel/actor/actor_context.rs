//! The view an actor gets while it handles one message.

use crate::kernel::actor::Actor;
use crate::kernel::actor::BehaviorStack;
use crate::kernel::actor::InvalidActorNameException;
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
  current_message: Option<Message>,
}

impl<'a> ActorContext<'a> {
  pub(crate) fn new(
    self_ref: ActorRef,
    sender: ActorRef,
    executor: &'a mut Executor,
    stack: &'a mut BehaviorStack,
  ) -> Self {
    Self { self_ref, sender, executor, stack, current_message: None }
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
  pub fn tell<M: core::any::Any + Send + Sync>(&mut self, target: &ActorRef, message: M) {
    self.executor.send(self.self_ref(), target, Message::new(message));
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
    self.current_message = Some(message.clone());
    let mut running = self.stack.take_running();
    match &mut running {
      | Running::Frame { frame, .. } => frame(self, message),
      | Running::Receive => initial.receive(self, message),
    }
    self.stack.finish_running(running);
    self.current_message = None;
  }

  /// Puts the message being handled aside, with its sender, until
  /// `unstash_all`.
  ///
  /// Mirrors `UnrestrictedStash.stash()`, which Pekko's `Stash` and the stash
  /// of `AbstractFSMWithStash` provide. The stash belongs to the actor, not to
  /// the behaviour, so it survives `change_behavior` and a state change.
  ///
  /// # Panics
  ///
  /// Panics outside the handling of a message, and when the same message is
  /// stashed twice in one delivery, as Pekko throws `IllegalStateException`.
  pub fn stash(&mut self) {
    let message =
      self.current_message.take().expect("Can't stash outside of message handling or the same message twice");
    let self_ref = self.self_ref();
    let sender = self.sender();
    self.executor.stash(self_ref, message, sender);
  }

  /// Returns every stashed message to the front of this actor's mailbox, in
  /// the order they were stashed.
  ///
  /// Mirrors `UnrestrictedStash.unstashAll()`, which prepends the stash to the
  /// mailbox: the stashed messages are handled before any message that is
  /// still queued for this actor, and after the current one returns.
  pub fn unstash_all(&mut self) {
    let self_ref = self.self_ref();
    self.executor.unstash_all(&self_ref);
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

  /// Starts a child of this actor under `name`, using this actor's strategy.
  ///
  /// Mirrors `ActorContext.actorOf(props, name)`. The child is placed under the
  /// strategy that this actor reports through
  /// `Actor::supervisor_strategy`. A name that a live child of this actor
  /// already uses is refused with [`InvalidActorNameException`], and a name is
  /// reusable once the child that held it has stopped. The name is checked as
  /// given; the port does not treat any prefix specially, so a caller that
  /// wants a generated name must generate one, as Pekko's
  /// `context.actorOf(props)` does.
  ///
  /// # Errors
  ///
  /// Returns [`InvalidActorNameException`] when the name is taken by a live
  /// child of this actor, or when this actor is terminating or terminated.
  pub fn actor_of(&mut self, props: Props, name: &str) -> Result<ActorRef, InvalidActorNameException> {
    let parent = self.self_ref();
    self.executor.spawn_named_child(&parent, props, name)
  }

  /// Registers this actor as a watcher of `target` and returns `target`'s
  /// handle.
  ///
  /// Mirrors `ActorContext.watch(ref)`, which returns `ref` so a caller can
  /// watch and keep the handle in one expression. The watcher receives a
  /// `Terminated` for `target` once it stops.
  pub fn watch(&mut self, target: &ActorRef) -> ActorRef {
    let watcher = self.self_ref();
    self.executor.watch(&watcher, target);
    target.clone()
  }

  /// Stops this actor from watching `target`.
  ///
  /// Mirrors `ActorContext.unwatch(ref)`, which returns `ref`. Removing a watch
  /// that was never registered changes nothing, as Pekko's `unwatch` is
  /// idempotent. Cancels a DeathWatch notice that is still queued for delivery
  /// to this actor.
  pub fn unwatch(&mut self, target: &ActorRef) -> ActorRef {
    let watcher = self.self_ref();
    self.executor.unwatch(&watcher, target);
    target.clone()
  }

  /// Returns a no-op reference with the same identity as a stopped local actor.
  ///
  /// Mirrors the colliding reference the original UidClash case watches in
  /// `postRestart`: `old`'s identifier, generation, and path over a delivery
  /// entity that ignores ordinary messages and does not answer a watch.
  ///
  /// # Panics
  /// Panics if `old` is live or does not address an actor on this shard.
  pub fn stopped_noop_ref(&self, old: &ActorRef) -> ActorRef {
    self.executor.stopped_noop_ref(old)
  }

  /// Returns whether `target` no longer has a running actor behind it.
  ///
  /// Mirrors `ActorRef.isTerminated` from inside an actor: a handle reports
  /// false while its actor runs and true once the actor has stopped. A handle
  /// to a restarted actor keeps reporting false after the restart. A handle of
  /// another system, or the reply address of an `ask`, has no cell here and
  /// reports true, the same limit `watch` documents.
  pub fn is_terminated(&self, target: &ActorRef) -> bool {
    self.executor.is_actor_terminated(target)
  }

  /// Asks `target` to stop.
  ///
  /// Mirrors `ActorContext.stop(ref)`. Pekko's stop is asynchronous: the
  /// target keeps running until the message that asked for the stop returns,
  /// so a child stopped here is still alive for the rest of the current
  /// delivery and a new child of the same name is refused until the stop has
  /// finished. The port records the request and the executor carries it out
  /// after the actor returns.
  pub fn stop(&mut self, target: &ActorRef) {
    self.executor.request_stop(target.clone());
  }

  /// Completes child stops before the default restart hook runs `post_stop`.
  pub(crate) fn stop_children(&mut self) {
    self.executor.stop_children_of(&self.self_ref);
  }

  /// Records the strategy this actor applies to its children.
  pub(crate) fn register_parent_strategy(&mut self, strategy: SupervisorStrategy) {
    let self_ref = self.self_ref();
    self.executor.register_parent_strategy(self_ref, strategy);
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
