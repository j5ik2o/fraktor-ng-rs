// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2014-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2017-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! An actor system and its lifecycle.

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use core::fmt;
use core::time::Duration;

use crate::kernel::actor::dispatch_settings::DispatchSettings;
use crate::kernel::actor::ActorSelection;
use crate::kernel::actor::AllDeadLetters;
use crate::kernel::actor::DeadLetter;
use crate::kernel::actor::DeadLetterLogLimit;
use crate::kernel::actor::DeadLetterSettings;
use crate::kernel::actor::DeadLetterSuppression;
use crate::kernel::actor::Dropped;
use crate::kernel::actor::FunctionRef;
use crate::kernel::actor::Props;
use crate::kernel::actor::SuppressedDeadLetter;
use crate::kernel::actor::UnhandledMessage;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_path::RootActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::actor_system::weak_actor_system::WeakActorSystem;
use crate::kernel::address::Address;
use crate::kernel::event::EventClass;
use crate::kernel::event::EventStream;
use crate::kernel::executor::Executor;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::setup::ActorSystemSetup;
use crate::kernel::supervision::FailureCatcher;

#[cfg(test)]
#[path = "actor_system_value_test.rs"]
mod tests;

/// The protocol of the address a local ported actor system answers on.
const LOCAL_PROTOCOL: &str = "pekko";

/// The root of an actor hierarchy, named by the caller at creation.
///
/// Mirrors `org.apache.pekko.actor.ActorSystem` for the lifecycle the ported
/// tests need: creating a system by name on a given shard boundary, starting
/// actors with `actor_of`, driving their messages to completion, stopping an
/// actor, and terminating the system. Dispatchers, guardians, and extensions
/// belong to later goals.
///
/// The executor sits behind an `Rc<RefCell<..>>` so that a [`WeakActorSystem`]
/// can reach it after the guardian is gone. A `Weak` is the only shared
/// pointer here; the executor stays singly owned by the system while the
/// system is alive.
///
/// Whether the system has been terminated is kept by the executor, so a handle
/// that reaches the system by its weak reference is refused in exactly the same
/// way as this value.
pub struct ActorSystem {
  name: String,
  executor: Rc<RefCell<Executor>>,
  event_stream: EventStream,
  setup: ActorSystemSetup,
}

impl ActorSystem {
  /// Creates a running actor system with the given name, whose actors share
  /// `boundary` as the edge of their shard.
  ///
  /// The system holds an empty [`ActorSystemSetup`].
  pub fn new(name: &str, boundary: Arc<dyn ShardBoundary>) -> Self {
    Self::with_setup(name, boundary, ActorSystemSetup::empty())
  }

  /// Creates a running actor system as [`new`](Self::new) does, holding
  /// `setup` for [`setup`](Self::setup) to return.
  ///
  /// Mirrors `ActorSystem(name, setup)`. Most setups only keep their
  /// values for later reads, but a setup can affect how the system
  /// runs: a [`DispatchSettings`] in the setup is installed on the
  /// executor, whose spawn-time resolution then bounds the stash and
  /// mailbox capacities of the actors it names.
  pub fn with_setup(name: &str, boundary: Arc<dyn ShardBoundary>, setup: ActorSystemSetup) -> Self {
    let executor = Rc::new(RefCell::new(Executor::new(Self::root_path(name), boundary)));
    let system = WeakActorSystem::new(name.to_string(), Rc::downgrade(&executor));
    let event_stream = EventStream::new(system, false);
    executor.borrow_mut().set_event_stream(event_stream.clone());
    if let Some(settings) = setup.get::<DispatchSettings>() {
      executor.borrow_mut().set_dispatch_settings(settings.clone());
    }
    executor.borrow().set_dead_letter_bus(event_stream.downgrade());
    // The four dead letter classes are subclasses of the AllDeadLetters
    // channel, as the trait links declare in Pekko.
    event_stream.declare_subclass::<DeadLetter, AllDeadLetters>();
    event_stream.declare_subclass::<SuppressedDeadLetter, AllDeadLetters>();
    event_stream.declare_subclass::<Dropped, AllDeadLetters>();
    event_stream.declare_subclass::<UnhandledMessage, AllDeadLetters>();
    Self { name: name.to_string(), executor, event_stream, setup }
  }

  /// Creates a running actor system with `setup`, as
  /// [`with_setup`](Self::with_setup) does, that also reports a failing
  /// actor to `catcher`, as
  /// [`with_failure_catcher`](Self::with_failure_catcher) does.
  pub fn with_setup_and_failure_catcher(
    name: &str,
    boundary: Arc<dyn ShardBoundary>,
    setup: ActorSystemSetup,
    catcher: Arc<dyn FailureCatcher>,
  ) -> Self {
    let system = Self::with_setup(name, boundary, setup);
    system.executor.borrow_mut().set_failure_catcher(catcher);
    system
  }

  /// Creates a running actor system that reports a failing actor to `catcher`.
  ///
  /// The catcher is what lets a parent apply a supervisor strategy: the kernel
  /// cannot catch a panic, so it asks the port. A system created with `new`
  /// has no catcher and lets a panic out, which keeps the behaviour of the
  /// earlier milestones.
  pub fn with_failure_catcher(name: &str, boundary: Arc<dyn ShardBoundary>, catcher: Arc<dyn FailureCatcher>) -> Self {
    let system = Self::new(name, boundary);
    system.executor.borrow_mut().set_failure_catcher(catcher);
    system
  }

  /// Installs `sink`, which receives the log events the system raises.
  ///
  /// Mirrors the role of Pekko's event stream for the failures of a faulty
  /// actor: a test attaches a sink to watch the `Error` events of a stopped or
  /// restarted child. The sink also receives a `Warning` for every message an
  /// actor reports through `ActorContext::unhandled`, in the place of Pekko's
  /// `TestEventListener`; the event stream gets the `UnhandledMessage` either
  /// way. A sink outside tests therefore sees one warning per unhandled
  /// message. A system created without a sink drops the events.
  pub fn set_event_sink(&self, sink: Arc<dyn crate::kernel::actor::Logging::EventSink>) {
    self.executor.borrow_mut().set_event_sink(sink);
  }

  /// Installs the monotonic clock a time-windowed supervisor strategy reads.
  ///
  /// Mirrors the `System.nanoTime` clock a Pekko `withinTimeRange` reads. The
  /// standard channel boundary supplies a clock. A custom boundary can supply
  /// its own time, or the caller can install this override before starting
  /// actors supervised with a finite window.
  pub fn set_clock(&self, clock: Arc<dyn crate::kernel::Clock>) {
    self.executor.borrow_mut().set_clock(clock);
  }

  fn root_path(name: &str) -> ActorPath {
    let root = RootActorPath::new(Address::new(LOCAL_PROTOCOL, name));
    ActorPath::from_root(root)
  }

  /// Returns the name the system was created with.
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Returns the setup the system was created with.
  ///
  /// Mirrors `system.settings.setup`. The port has no settings layer, so the
  /// system returns its setup directly. A system created with
  /// [`new`](Self::new) returns an empty setup.
  pub fn setup(&self) -> &ActorSystemSetup {
    &self.setup
  }

  /// Starts the actor a recipe describes and returns a handle to it.
  ///
  /// Each call hands out a handle with a fresh identifier and generation zero,
  /// so handles of different actors compare unequal. The actor is started below
  /// `/user`, with the default supervisor strategy, as the guardian does for a
  /// top-level actor.
  ///
  /// Panics once the system has been terminated, with the same cause as Pekko,
  /// which throws `IllegalStateException` from `attachChild` when a defunct
  /// guardian cannot create children.
  pub fn actor_of(&mut self, props: Props) -> ActorRef {
    self.executor.borrow_mut().spawn_user(props)
  }

  /// Stops `target`, telling its watchers that it is gone.
  ///
  /// Mirrors `org.apache.pekko.actor.ActorSystem.stop`, which asks the
  /// guardian to stop the actor.
  pub fn stop(&mut self, target: &ActorRef) {
    self.executor.borrow_mut().stop_at_boundary(target);
  }

  /// Registers `watcher` to receive the `Terminated` notice of `target`.
  ///
  /// Mirrors `ActorContext.watch` with the scope of the system's guardian. The
  /// scope is the actors of this system and the function references that this
  /// system made for its test actors. A target outside that scope, such as an
  /// actor of another system, a reference that has already stopped, or the
  /// reply address of an `ask`, is reported at once with `existence_confirmed =
  /// false`; no later notice arrives for it, even if it stops afterwards.
  pub fn watch(&mut self, watcher: &ActorRef, target: &ActorRef) {
    self.executor.borrow_mut().watch(watcher, target);
  }

  /// Returns whether `target` no longer has a running actor behind it.
  ///
  /// Mirrors `ActorRef.isTerminated` for the actors and function references of
  /// this system: a handle reports false while its actor runs and true once the
  /// actor has stopped. A handle to a restarted actor keeps reporting false
  /// after the restart, because the restarted cell answers for the same
  /// identifier and generation. A handle of another system, or the reply
  /// address of an `ask`, has no cell here and reports true, the same limit
  /// `watch` documents.
  pub fn is_actor_terminated(&self, target: &ActorRef) -> bool {
    self.executor.borrow().is_actor_terminated(target)
  }

  /// Starts a handle whose shard runs `handler` for each delivered message.
  ///
  /// This is how the port expresses Pekko's test actor, which runs on the
  /// calling-thread dispatcher: the handle shares no cell, and posting to it
  /// runs the handler before the sender returns. The path is
  /// `<root>/system/<name>`, mirroring `systemActorOf`.
  ///
  /// Panics once the system has been terminated, as `actor_of` does: Pekko's
  /// `systemActorOf` asks the guardian for a child, so a defunct system refuses
  /// it with the same cause.
  pub fn function_ref<H>(&mut self, name: &str, handler: H) -> ActorRef
  where
    H: Fn(&ActorRef, crate::kernel::message::Message) + Send + Sync + 'static,
  {
    self.executor.borrow_mut().function_ref(name, handler)
  }

  /// Registers the function reference `watcher` to receive the
  /// `Terminated` of `target` as a message to its function.
  ///
  /// Mirrors `FunctionRef.watch`, lent through the system because the
  /// handle itself holds no executor reference (design of #192, H1).
  pub fn function_ref_watch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef {
    self.executor.borrow_mut().function_ref_watch(watcher, target)
  }

  /// Stops the function reference `watcher` from watching `target`.
  ///
  /// Mirrors `FunctionRef.unwatch`, which is idempotent.
  pub fn function_ref_unwatch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef {
    self.executor.borrow_mut().function_ref_unwatch(watcher, target)
  }

  /// Returns whether the function reference `watcher` watches `target`.
  ///
  /// Mirrors `FunctionRef.isWatching`: the registration survives the
  /// target's end until the reference unwatches or stops.
  pub fn function_ref_is_watching(&self, watcher: &FunctionRef, target: &ActorRef) -> bool {
    self.executor.borrow().function_ref_is_watching(watcher, target)
  }

  /// Returns a handle that can reach the executor without owning the system.
  pub fn downgrade(&self) -> WeakActorSystem {
    WeakActorSystem::new(self.name.clone(), Rc::downgrade(&self.executor))
  }

  /// Returns the event stream of the system.
  ///
  /// Mirrors `ActorSystem.eventStream`: the returned handle shares the
  /// subscribers of this system, so subscribing to it reaches the same stream
  /// that the system publishes its own events to.
  pub fn event_stream(&self) -> EventStream {
    self.event_stream.clone()
  }

  /// Registers the payload type `T` for dead letter suppression.
  ///
  /// Mirrors the run-time half of Pekko's `DeadLetterSuppression` marker
  /// trait: Rust cannot test trait membership at run time, so the system
  /// records the type's identity in the table the dead letter routing
  /// rules read. A message whose payload is a registered type is reported
  /// as a `SuppressedDeadLetter` and never counted by the dead letter
  /// listener.
  pub fn register_dead_letter_suppression<T: core::any::Any + DeadLetterSuppression>(&self) {
    self.executor.borrow().register_dead_letter_suppression(core::any::TypeId::of::<T>());
  }

  /// Starts the dead letter listener with `settings` and returns its
  /// handle.
  ///
  /// Mirrors the `DeadLetterListener` a Pekko system starts at start-up
  /// when `pekko.log-dead-letters` is not off. The port has no
  /// configuration layer for the two settings, so the caller hands them in
  /// as a value; a system that never calls this starts no listener, and
  /// dead letters are still published to the event stream.
  pub fn start_dead_letter_listener(&mut self, settings: DeadLetterSettings) -> ActorRef {
    // The subscription is made here, after the executor borrow is
    // released: subscribing registers the listener with the unsubscriber,
    // whose watch borrows the executor again and would panic on the
    // borrow the executor's own start still holds. With the limit `Off`
    // the listener is not subscribed at all (design L7): Pekko's listener
    // with dead letter logging off never receives an event, so the handle
    // is created and returned without a subscription.
    let limit = settings.limit();
    let listener = self.executor.borrow_mut().start_dead_letter_listener(settings);
    if limit != DeadLetterLogLimit::Off {
      let _ = self.event_stream.subscribe(Some(listener.clone()), EventClass::of::<DeadLetter>());
      let _ = self.event_stream.subscribe(Some(listener.clone()), EventClass::of::<Dropped>());
      let _ = self.event_stream.subscribe(Some(listener.clone()), EventClass::of::<UnhandledMessage>());
    }
    listener
  }

  /// Returns a selection that resolves `path` when a message is delivered.
  ///
  /// Mirrors `ActorSystem.actorSelection`. The selection captures the
  /// shard boundary, so it never borrows the executor when it sends.
  pub fn actor_selection(&self, path: &ActorPath) -> ActorSelection {
    self.executor.borrow().actor_selection(path.clone())
  }

  /// Returns the handle that absorbs messages which no actor takes.
  ///
  /// Mirrors `ActorSystem.deadLetters`. Pekko names the handle below the root
  /// path; the port makes one when the system is created and answers that same
  /// handle afterwards.
  pub fn dead_letters(&self) -> ActorRef {
    self.executor.borrow().dead_letters()
  }

  /// Delivers every message that is waiting until nothing is left.
  pub fn run_until_idle(&mut self) {
    self.executor.borrow_mut().run_until_idle();
  }

  /// Delivers at most one message and returns whether it delivered one, so a
  /// caller can check a deadline between messages instead of waiting for the
  /// whole queue to drain.
  pub fn run_one(&mut self) -> bool {
    self.executor.borrow_mut().run_one()
  }

  /// Waits at most `max` for one message from the boundary and moves it onto
  /// the local queue without delivering it; call `run_until_idle` to deliver
  /// it. Returns whether a message arrived.
  pub fn await_envelope(&mut self, max: Duration) -> bool {
    self.executor.borrow_mut().await_envelope(max)
  }

  /// Stops the system. Calling it again is a no-op, as in Pekko.
  pub fn terminate(&mut self) {
    if !self.is_terminated() {
      self.executor.borrow_mut().stop_all();
    }
  }

  /// Returns whether the system has been terminated.
  pub fn is_terminated(&self) -> bool {
    self.executor.borrow().is_terminated()
  }

  /// Returns the identity of the shard boundary this system posts to.
  ///
  /// A waiting `ask` uses the same identity, taken from the recipient handle,
  /// to find the step that drives this system.
  pub fn shard_key(&self) -> usize {
    self.executor.borrow().boundary_key()
  }

  /// Returns the event sink the system publishes to, if one was installed.
  ///
  /// The test kit reads it back to arm an `EventFilter` for the duration of an
  /// `intercept`.
  pub fn event_sink(&self) -> Option<Arc<dyn crate::kernel::actor::Logging::EventSink>> {
    self.executor.borrow().event_sink()
  }
}

impl Drop for ActorSystem {
  fn drop(&mut self) {
    self.terminate();
  }
}

impl fmt::Debug for ActorSystem {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.debug_struct("ActorSystem").field("name", &self.name).field("terminated", &self.is_terminated()).finish()
  }
}
