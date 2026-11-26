// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2017-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2014-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The view an actor gets while it handles one message.

use crate::kernel::actor::Actor;
use crate::kernel::actor::BehaviorStack;
use crate::kernel::actor::FunctionRef;
use crate::kernel::actor::InvalidActorNameException;
use crate::kernel::actor::Props;
use crate::kernel::actor::Running;
use crate::kernel::actor::StashOverflowException;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::Executor;
use crate::kernel::message::Message;
use crate::kernel::supervision::SupervisorStrategy;
use alloc::boxed::Box;

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
  current_message: Option<(Message, u64)>,
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

  /// Reports `message` as one this actor did not handle.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.unhandled`, which an actor calls
  /// from `receive` for a message its behaviour does not cover. Pekko publishes
  /// `UnhandledMessage(message, sender(), self)` on the event stream, and the
  /// port does the same through the executor. The executor also offers the
  /// report to the event sink that stands in for Pekko's `TestEventListener`,
  /// as a `Warning` reading `unhandled message from <sender>: <message>`.
  ///
  /// Pekko's other branch, which throws `DeathPactException` for a
  /// [`Terminated`](crate::kernel::actor::Terminated), is not ported yet: it
  /// needs the exception type and the supervision policy that stops on it.
  /// Until then a `Terminated` given here is reported like any other message
  /// and the actor keeps running, so a port of `DeathWatchSpec`'s "fail a
  /// monitor which does not handle Terminated()" cannot pass. Issue #219
  /// (<https://github.com/j5ik2o/fraktor-ng-rs/issues/219>) changes this.
  pub fn unhandled<M: core::any::Any + Send + Sync>(&mut self, message: M) {
    let message = Message::new(message);
    let sender = self.sender();
    let recipient = self.self_ref();
    self.executor.publish_unhandled(message, sender, recipient);
  }

  /// Returns the dead letters handle of the system.
  ///
  /// Mirrors `context.system.deadLetters`. An actor that reports a message
  /// it dropped on purpose names this handle as the sender of the report,
  /// which is the port's convention for a report with no real sender.
  pub fn dead_letters(&self) -> ActorRef {
    self.executor.dead_letters()
  }

  /// Publishes `event` on the event stream of the system.
  ///
  /// Mirrors `context.system.eventStream.publish(event)`, which is how an
  /// actor reports a [`Dropped`](crate::kernel::actor::Dropped) message.
  pub fn publish_event<E: core::any::Any + Send + Sync>(&mut self, event: E) {
    self.executor.publish_event(event);
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
    F: FnMut(&mut ActorContext<'_>, Message) + Send + 'static,
  {
    self.stack.change(Box::new(behavior));
  }

  /// Installs `behavior` above the current behaviour without dropping it.
  ///
  /// Mirrors `context.become(behavior, discardOld = false)`. A later
  /// `pop_behavior` restores the behaviour below.
  pub fn push_behavior<F>(&mut self, behavior: F)
  where
    F: FnMut(&mut ActorContext<'_>, Message) + Send + 'static,
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
    self.current_message = Some((message.clone(), self.executor.next_delivery_id()));
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
  /// A restart returns the stash to the front of the mailbox before the old
  /// instance's `pre_restart`, so the new instance handles it, as
  /// `UnrestrictedStash.preRestart` does. A stop sends the stash to dead
  /// letters with each message's original sender, as Pekko's mailbox clean-up
  /// does after `UnrestrictedStash.postStop`; so does a stash that the stop
  /// hook or a restart whose replacement fails returned to the mailbox. The
  /// executor publishes a `DeadLetter` event for those messages.
  /// Terminating the system releases every stash with the other queues.
  /// The stash capacity comes from the actor's dispatcher and mailbox
  /// definitions in the system's
  /// [`DispatchSettings`](crate::kernel::actor::dispatch_settings::DispatchSettings): when a
  /// capacity is set and the stash is full, `stash` panics with
  /// [`StashOverflowException`], as Pekko's throw reaches the supervisor;
  /// [`try_stash`](Self::try_stash) answers with the error instead.
  ///
  /// # Panics
  ///
  /// Panics outside the handling of a message. Panics as well when the message
  /// being handled is still the newest entry of the stash, as Pekko throws
  /// `IllegalStateException("Can't stash the same message ... more than once")`.
  /// Pekko compares the delivery with the last stashed one, so a delivery may
  /// stash again after `unstash_all` has emptied the stash; the port does the
  /// same.
  pub fn stash(&mut self) {
    let (message, delivery) = self.current_message.clone().expect("Can't stash outside of message handling");
    let self_ref = self.self_ref();
    let sender = self.sender();
    self.executor.stash(self_ref, delivery, message, sender);
  }

  /// Returns every stashed message to the front of this actor's mailbox, in
  /// the order they were stashed.
  ///
  /// Mirrors `UnrestrictedStash.unstashAll()`, which prepends the stash to the
  /// mailbox: the stashed messages are handled before any message that is
  /// still queued for this actor, and after the current one returns. The port
  /// keeps one queue per shard, so they also go ahead of messages queued for
  /// other actors of the same shard; Pekko's per-actor mailboxes promise no
  /// order between actors.
  ///
  /// A stashed `Terminated` is queued again as a DeathWatch notice, as Pekko's
  /// `enqueueFirst` does: the actor receives it again, and an `unwatch` of its
  /// target before it is handled cancels it. Notices for one target share one
  /// record, as Pekko keys `terminatedQueued` by the target, so the first one
  /// handled consumes it and the others are dropped.
  pub fn unstash_all(&mut self) {
    let self_ref = self.self_ref();
    self.executor.unstash_all(&self_ref);
  }

  /// Puts the message being handled aside, as [`stash`](Self::stash)
  /// does, answering with [`StashOverflowException`] when the stash is
  /// already at the capacity the actor's definitions set.
  ///
  /// Mirrors catching Pekko's `StashOverflowException` around `stash()`:
  /// the original's overflow actor counts the calls and reacts to the
  /// exception itself, which a panic cannot do in this port because
  /// `receive` has no return value and a panic becomes a supervisor
  /// decision. The state does not change when this answers with the
  /// error. The double-stash contract of [`stash`](Self::stash) still
  /// panics, as it is a contract violation rather than a capacity one.
  ///
  /// # Errors
  ///
  /// Returns [`StashOverflowException`] when the stash is at capacity.
  ///
  /// # Panics
  ///
  /// Panics outside the handling of a message, and on a double stash,
  /// as [`stash`](Self::stash) documents.
  pub fn try_stash(&mut self) -> Result<(), StashOverflowException> {
    let (message, delivery) = self.current_message.clone().expect("Can't stash outside of message handling");
    let self_ref = self.self_ref();
    let sender = self.sender();
    self.executor.try_stash(self_ref, delivery, message, sender)
  }

  /// Registers a function on this actor's cell and returns its handle.
  ///
  /// Mirrors `ActorCell.addFunctionRef`: the reference lives below this
  /// actor's path, runs `handler` for each message sent to it with the
  /// message's sender, ends when this actor ends, and can be removed
  /// with [`remove_function_ref`](Self::remove_function_ref).
  pub fn add_function_ref<H>(&mut self, handler: H) -> FunctionRef
  where
    H: Fn(&ActorRef, Message) + Send + Sync + 'static,
  {
    let parent = self.self_ref();
    self.executor.add_function_ref(&parent, handler)
  }

  /// Removes a function reference this actor created with
  /// [`add_function_ref`](Self::add_function_ref).
  ///
  /// Mirrors `ActorCell.removeFunctionRef`: the reference's watchers
  /// receive `Terminated` and later messages to it go to dead letters.
  /// Removing one's own reference again changes nothing.
  ///
  /// # Panics
  ///
  /// Panics when `target` is a function reference of another actor, as
  /// the original's `require` fails the actor in that case.
  pub fn remove_function_ref(&mut self, target: &FunctionRef) {
    let parent = self.self_ref();
    self.executor.remove_function_ref(&parent, target);
  }

  /// Registers the function reference `watcher` to receive the
  /// `Terminated` of `target` as a message to its function.
  ///
  /// Mirrors `FunctionRef.watch`, lent through the context because the
  /// handle itself holds no executor reference.
  /// The registration survives the notice until
  /// [`function_ref_unwatch`](Self::function_ref_unwatch) or the
  /// reference's own stop.
  pub fn function_ref_watch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef {
    self.executor.function_ref_watch(watcher, target)
  }

  /// Stops the function reference `watcher` from watching `target`.
  ///
  /// Mirrors `FunctionRef.unwatch`, which is idempotent.
  pub fn function_ref_unwatch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef {
    self.executor.function_ref_unwatch(watcher, target)
  }

  /// Returns whether the function reference `watcher` watches `target`.
  ///
  /// Mirrors `FunctionRef.isWatching`.
  pub fn function_ref_is_watching(&self, watcher: &FunctionRef, target: &ActorRef) -> bool {
    self.executor.function_ref_is_watching(watcher, target)
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
  /// This entry checks the name only: unlike
  /// [`WeakActorSystem::try_spawn_child`](crate::kernel::actor_system::WeakActorSystem::try_spawn_child),
  /// it does not verify the recipe's configuration, so a dispatcher name
  /// that no configuration defines is accepted here and refused there.
  /// That difference is a known deviation; issue #247
  /// (https://github.com/j5ik2o/fraktor-ng-rs/issues/247) aligns this
  /// entry with `try_spawn_child` so both verify the configuration.
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

  // The `watch` above panics when the pair already carries a custom
  // message from `watch_with`, as Pekko's `checkWatchingSame` throws.

  /// Registers this actor as a watcher of `target`, with `message` as
  /// the notice instead of the plain [`Terminated`](crate::kernel::actor::Terminated).
  ///
  /// Mirrors `ActorContext.watchWith(ref, message)`, which returns `ref`.
  /// One (watcher, target) pair carries one notice: calling `watch_with`
  /// with a different message for a target this actor already watches,
  /// or calling [`watch`](Self::watch) for a target watched with a
  /// custom message, is a contract violation and panics, as Pekko's
  /// `checkWatchingSame` throws `IllegalStateException`. Repeating the
  /// same registration is idempotent. Message equality is by payload
  /// type, the strongest comparison the port's erased `Message` offers.
  ///
  /// # Panics
  ///
  /// Panics when the pair already carries a different notice.
  pub fn watch_with<M: core::any::Any + Send + Sync>(&mut self, target: &ActorRef, message: M) -> ActorRef {
    let watcher = self.self_ref();
    self.executor.watch_with(&watcher, target, Message::new(message));
    target.clone()
  }

  /// Returns whether this actor watches `target`.
  ///
  /// Mirrors `ActorContext.isWatching(ref)`.
  pub fn is_watching(&self, target: &ActorRef) -> bool {
    let watcher = self.self_ref();
    self.executor.is_watching(&watcher, target)
  }

  /// Sends an already-built `message` to `target`, naming `sender`.
  pub(crate) fn tell_message(&mut self, target: &ActorRef, message: Message, sender: ActorRef) {
    self.executor.send(sender, target, message);
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
