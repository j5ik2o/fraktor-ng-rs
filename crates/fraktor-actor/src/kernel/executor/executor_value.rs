// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2014-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2017-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The executor that owns one shard of actors and runs their messages.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::time::Duration;

use portable_atomic::AtomicU64;
use portable_atomic::Ordering;

use crate::kernel::actor::dispatch_settings::CapacityLimit;
use crate::kernel::actor::dispatch_settings::DispatchSettings;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::ActorSelection;
use crate::kernel::actor::ActorSelectionMessage;
use crate::kernel::actor::AnyDeadLetter;
use crate::kernel::actor::BehaviorStack;
use crate::kernel::actor::DeadLetter;
use crate::kernel::actor::DeadLetterSettings;
use crate::kernel::actor::DeadLettersShared;
use crate::kernel::actor::FunctionRef;
use crate::kernel::actor::InvalidActorNameException;
use crate::kernel::actor::Logging::Error;
use crate::kernel::actor::Logging::EventSink;
use crate::kernel::actor::Logging::Info;
use crate::kernel::actor::Logging::Warning;
use crate::kernel::actor::Props;
use crate::kernel::actor::StashOverflowException;
use crate::kernel::actor::Terminated;
use crate::kernel::actor::UnhandledMessage;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::event::DeadLetterListener;
use crate::kernel::event::EventStream;
use crate::kernel::executor::ActorCell;
use crate::kernel::executor::FunctionBoundary;
use crate::kernel::executor::ShardQueue;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::supervision::Directive;
use crate::kernel::supervision::FailureCatcher;
use crate::kernel::supervision::FailureKind;
use crate::kernel::supervision::RestartHistory;
use crate::kernel::supervision::RestartLimit;
use crate::kernel::supervision::RunOutcome;
use crate::kernel::supervision::SupervisorStrategy;
use crate::kernel::sync::Arc;
use crate::kernel::sync::Weak;
use crate::kernel::Clock;

#[cfg(test)]
#[path = "executor_value_test.rs"]
mod tests;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_SPAWN: AtomicU64 = AtomicU64::new(0);

/// The cause name Pekko reports when `Kill` ends an actor.
const ACTOR_KILLED_CAUSE: &str = "ActorKilledException";

/// The cause name Pekko reports when an actor fails during `preStart`.
const ACTOR_INITIALIZATION_CAUSE: &str = "ActorInitializationException";

/// The cause name Pekko reports when a message handler throws.
const RUNTIME_CAUSE: &str = "RuntimeException";

/// The cause name Pekko reports when `postStop` throws.
const EXCEPTION_CAUSE: &str = "Exception";

/// The message a `Kill` carries into the failure of the instance it ends.
const ACTOR_KILLED_MESSAGE: &str = "Kill";

/// The `PoisonPill` and `Kill` system messages, which the executor handles
/// without delivering them to `receive`.
const POISON_PILL: &str = "PoisonPill";
const KILL: &str = "Kill";

/// The log class the ported tests pass for an event an actor raises.
const ACTOR_LOG_CLASS: i32 = 3;

/// Creates one handle with a fresh identifier and generation zero.
pub(crate) fn next_actor_ref(path: ActorPath, boundary: Arc<dyn crate::kernel::executor::ShardBoundary>) -> ActorRef {
  let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  ActorRef::new(id, 0, Arc::new(path), boundary)
}

/// Returns the next number from the counter that names children without a name.
pub(crate) fn next_spawn_name() -> u64 {
  NEXT_SPAWN.fetch_add(1, Ordering::Relaxed)
}

/// The cause Pekko throws when a defunct guardian is asked for a child.
///
/// Mirrors the `IllegalStateException` of `Children.attachChild`.
pub(crate) const TERMINATED_MESSAGE: &str = "cannot create children while terminating or terminated";

/// The cause name Pekko reports when the child-terminated hook fails.
///
/// Mirrors the `IllegalStateException("handleChildTerminated failed")` that
/// `FaultHandling.handleChildTerminated` logs when the hook throws while its
/// actor is terminating.
const HANDLE_CHILD_TERMINATED_FAILED_CAUSE: &str = "IllegalStateException";

/// The message Pekko attaches to the logged failure of the child-terminated
/// hook.
const HANDLE_CHILD_TERMINATED_FAILED_MESSAGE: &str = "handleChildTerminated failed";

/// A reply to send to `reply_to` after the actor it was registered on stops.
struct DeferredReply {
  reply_to: ActorRef,
  message: Message,
}

/// One parent and the facts the executor keeps on its behalf.
///
/// Mirrors the `Children` container that a Pekko `ActorCell` holds for one
/// actor: the strategy the parent applies to its children, the names its live
/// children use, and whether the parent is shutting down. The executor keeps
/// this apart from the parent's cell because the cell is out of the table
/// while the parent handles a message, yet the parent may still ask for a
/// child inside that delivery.
struct ParentEntry {
  parent: ActorRef,
  strategy: SupervisorStrategy,
  children: Vec<(String, ActorRef)>,
  stopping: bool,
}

/// One callback that the executor runs with the handle of a stopped actor.
type TerminationCallback = Box<dyn Fn(&ActorRef)>;

/// The notice the dead letter listener's handler posts to the shard
/// boundary when its state machine stops itself.
///
/// The handler runs on a sender's stack and must not borrow the
/// executor, so it cannot stop its own reference there. It posts this
/// envelope to the boundary instead, and the delivery of the notice is
/// what carries the stop out, so a limit reached outside any delivery
/// (a direct tell to the dead letters handle, a stash a stop releases)
/// is not lost.
struct StopDeadLetterListener;

/// A DeathWatch notice that can still be cancelled before local delivery.
struct PendingTermination {
  watcher: ActorRef,
  notice: Terminated,
}

/// Creates actors of one shard, runs their messages, and owns their state.
///
/// Mirrors the role of `org.apache.pekko.actor.ActorCell` together with a
/// dispatcher, reduced to the temporary policy of this port: one shard per
/// executor, a local queue inside the shard, and a `ShardBoundary` only at the
/// edge. The executor alone owns the cells; a caller only holds `ActorRef`
/// handles.
///
/// The executor also owns whether the system has been terminated. Keeping that
/// one fact here lets every entry that starts a new actor, including the ones
/// that reach the executor through a weak handle, refuse the request with the
/// same cause as Pekko.
pub(crate) struct Executor {
  cells: Vec<ActorCell>,
  queue: ShardQueue,
  root: ActorPath,
  watchers: Vec<(ActorRef, ActorRef)>,
  pending_terminations: Vec<Arc<PendingTermination>>,
  function_refs: Vec<ActorRef>,
  catcher: Option<Arc<dyn FailureCatcher>>,
  event_sink: Arc<spin::Mutex<Option<Arc<dyn EventSink>>>>,
  event_stream: Option<EventStream>,
  deferred_replies: Vec<(ActorRef, DeferredReply)>,
  parents: Vec<ParentEntry>,
  pending_stops: Vec<ActorRef>,
  pending_decider_failures: Vec<(ActorRef, String)>,
  pending_child_terminated: Vec<(ActorRef, ActorRef)>,
  running: Vec<ActorRef>,
  termination_callbacks: Vec<TerminationCallback>,
  dead_letters: ActorRef,
  dead_letters_shared: Arc<DeadLettersShared>,
  dead_letter_listener: Option<ActorRef>,
  boundary_active: bool,
  hook_child_creation_refused: bool,
  terminated: bool,
  clock: Option<Arc<dyn Clock>>,
  stashes: Vec<(ActorRef, Vec<(u64, Envelope)>)>,
  next_delivery: u64,
  dispatch_settings: Option<DispatchSettings>,
  actor_capacities: Vec<(ActorRef, CapacityLimit, CapacityLimit)>,
  function_parents: Vec<(ActorRef, ActorRef)>,
  function_watching: Vec<(ActorRef, ActorRef)>,
}

impl Executor {
  pub(crate) fn new(root: ActorPath, boundary: Arc<dyn crate::kernel::executor::ShardBoundary>) -> Self {
    let dead_letters_shared = Arc::new(DeadLettersShared::new());
    let dead_letters = Self::create_dead_letters(&root, dead_letters_shared.clone());
    let mut executor = Self {
      cells: Vec::new(),
      queue: ShardQueue::new(boundary),
      root,
      watchers: Vec::new(),
      pending_terminations: Vec::new(),
      function_refs: Vec::new(),
      catcher: None,
      event_sink: Arc::new(spin::Mutex::new(None)),
      event_stream: None,
      deferred_replies: Vec::new(),
      parents: Vec::new(),
      pending_stops: Vec::new(),
      pending_decider_failures: Vec::new(),
      pending_child_terminated: Vec::new(),
      running: Vec::new(),
      termination_callbacks: Vec::new(),
      dead_letters,
      dead_letters_shared,
      dead_letter_listener: None,
      boundary_active: false,
      hook_child_creation_refused: false,
      terminated: false,
      clock: None,
      stashes: Vec::new(),
      next_delivery: 0,
      dispatch_settings: None,
      actor_capacities: Vec::new(),
      function_parents: Vec::new(),
      function_watching: Vec::new(),
    };
    executor.function_refs.push(executor.dead_letters.clone());
    executor
  }

  /// Registers `callback` to run with the handle of every actor that stops.
  ///
  /// Mirrors the notification that Pekko's `ActorClassificationUnsubscriber`
  /// receives for a watched subscriber: the executor is the only place that
  /// knows an actor has stopped, so a component that is not an actor registers
  /// here. The callback runs after the watchers have been told and must not
  /// borrow the executor, which is already borrowed by the caller.
  pub(crate) fn on_termination<F>(&mut self, callback: F)
  where
    F: Fn(&ActorRef) + 'static,
  {
    self.termination_callbacks.push(Box::new(callback));
  }

  /// Returns the handle that absorbs messages which no actor takes.
  ///
  /// Mirrors `ActorSystem.deadLetters`. The handle posts to a boundary whose
  /// handler routes the message through the dead letter rules, publishing
  /// the event the rules name. The executor makes one such handle when it is
  /// created, below the root path, and answers that same handle for the life
  /// of the system.
  pub(crate) fn dead_letters(&self) -> ActorRef {
    self.dead_letters.clone()
  }

  fn create_dead_letters(root: &ActorPath, shared: Arc<DeadLettersShared>) -> ActorRef {
    let path = root.with_child("deadLetters");
    // The handler routes through the shared state, which the executor fills
    // with the event stream's table once the system is built. The routing
    // rules name the handle itself as the processing reference, so the
    // handler rebuilds it from the identifier, the path, and the boundary.
    // The boundary is held weakly: a strong slot would cycle handle ->
    // boundary -> handler -> handle, and dropping a system would leak the
    // boundary, the handler, and the shared routing state with it. The
    // slot is filled in once, here, so it sits in a `spin::Once` that the
    // handler reads without a lock, and it holds the path in an `Arc`, so
    // rebuilding the handle for one message clones no path.
    type HandleParts = (u64, Arc<ActorPath>, Weak<dyn crate::kernel::executor::ShardBoundary>);
    let slot: Arc<spin::Once<HandleParts>> = Arc::new(spin::Once::new());
    let slot_in_handler = slot.clone();
    let handler = Arc::new(move |sender: &ActorRef, message: Message| {
      if let Some((id, path, boundary)) = slot_in_handler.get() {
        if let Some(boundary) = boundary.upgrade() {
          let dead_letters = ActorRef::new(*id, 0, path.clone(), boundary);
          shared.route_at_dead_letters(message, sender.clone(), &dead_letters);
        }
      }
    });
    let boundary: Arc<dyn crate::kernel::executor::ShardBoundary> =
      Arc::new(crate::kernel::executor::FunctionBoundary::new(handler));
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let path = Arc::new(path);
    let reference = ActorRef::new(id, 0, path.clone(), boundary.clone());
    slot.call_once(|| (id, path, Arc::downgrade(&boundary)));
    reference
  }

  /// Records the subscription table the dead letter routing rules publish
  /// to, once the system has built its event stream.
  pub(crate) fn set_dead_letter_bus(&self, bus: crate::kernel::event::EventStreamBusWeak) {
    self.dead_letters_shared.set_bus(bus);
  }

  /// Registers a payload type for dead letter suppression.
  ///
  /// Mirrors the run-time half of Pekko's `DeadLetterSuppression` marker:
  /// Rust cannot test trait membership at run time, so the system records
  /// the type's identity here.
  pub(crate) fn register_dead_letter_suppression(&self, id: core::any::TypeId) {
    self.dead_letters_shared.register_suppression(id);
  }

  /// Publishes `event` on the event stream of the system, if one is set.
  pub(crate) fn publish_event<E: core::any::Any + Send + Sync>(&self, event: E) {
    if let Some(stream) = self.event_stream.as_ref() {
      stream.publish(event);
    }
  }

  /// Creates a selection for `path` on this shard.
  ///
  /// The selection captures the shard boundary and the dead letters handle
  /// (which carries envelopes addressed to it), so it never borrows the
  /// executor when it sends.
  pub(crate) fn actor_selection(&self, path: ActorPath) -> ActorSelection {
    ActorSelection::new(self.queue.boundary().clone(), self.dead_letters.clone(), path)
  }

  /// Starts the dead letter listener with `settings` and returns its
  /// handle.
  ///
  /// Mirrors the `DeadLetterListener` a Pekko system starts when
  /// `pekko.log-dead-letters` is not off: a function reference subscribed
  /// to the `DeadLetter`, `Dropped`, and `UnhandledMessage` channels,
  /// whose handler feeds the counting state machine and publishes the
  /// `Info` it produces to the event stream and the event sink. The
  /// handler captures the clock, the bus, the sink, and the dead letters
  /// handle at start, so it never borrows the executor.
  pub(crate) fn start_dead_letter_listener(&mut self, settings: DeadLetterSettings) -> ActorRef {
    let shared = self.dead_letters_shared.clone();
    let dead_letters = self.dead_letters.clone();
    let clock = self.clock.clone();
    let boundary = self.queue.boundary().clone();
    let now_of = Arc::new(move || -> Duration {
      match clock.as_ref() {
        | Some(clock) => clock.monotonic(),
        | None => boundary.monotonic_time().unwrap_or(Duration::ZERO),
      }
    });
    let bus = self.event_stream.as_ref().map(|stream| stream.downgrade());
    let sink = self.event_sink.clone();
    let state = Arc::new(spin::Mutex::new(DeadLetterListener::new(settings, dead_letters.clone())));
    let post_boundary = self.queue.boundary().clone();
    let listener = self.function_ref("DeadLetterListener", move |_: &ActorRef, message: Message| {
      let Some(event) = AnyDeadLetter::from_message(&message) else {
        return;
      };
      let is_suppressed = |candidate: &Message| shared.is_suppressed(candidate);
      let (text, stopped) = {
        let mut state = state.lock();
        let text = state.on_event(&event, now_of(), &is_suppressed);
        let stopped = text.is_some() && state.is_stopped();
        (text, stopped)
      };
      if let Some(text) = text {
        let info = Info::new(event.recipient().path().to_string(), 0, &text);
        if let Some(bus) = bus.as_ref() {
          bus.publish(info.clone());
        }
        let current = sink.lock().clone();
        if let Some(current) = current {
          current.publish(&info);
        }
      }
      if stopped {
        // The state machine stopped itself with this event: Pekko's
        // listener calls `context.stop(self)` here. This handler runs on
        // a sender's stack and must not borrow the executor, so it posts
        // the stop notice to the shard boundary instead, and the
        // delivery of that notice carries the stop out.
        post_boundary.post(Envelope::new(
          dead_letters.clone(),
          Message::new(StopDeadLetterListener),
          dead_letters.clone(),
        ));
      }
    });
    self.dead_letter_listener = Some(listener.clone());
    listener
  }

  pub(crate) fn function_ref<H>(&mut self, name: &str, handler: H) -> ActorRef
  where
    H: Fn(&ActorRef, Message) + Send + Sync + 'static,
  {
    self.assert_running();
    let path = self.root.with_child("system").with_child(name);
    let boundary: Arc<dyn crate::kernel::executor::ShardBoundary> =
      Arc::new(crate::kernel::executor::FunctionBoundary::new(Arc::new(handler)));
    let reference = next_actor_ref(path, boundary);
    self.function_refs.push(reference.clone());
    reference
  }

  pub(crate) fn set_failure_catcher(&mut self, catcher: Arc<dyn FailureCatcher>) {
    self.catcher = Some(catcher);
  }

  pub(crate) fn set_event_sink(&mut self, sink: Arc<dyn EventSink>) {
    *self.event_sink.lock() = Some(sink);
  }

  /// Installs the monotonic clock that a time-windowed strategy reads.
  ///
  /// Mirrors the `System.nanoTime` clock a Pekko `withinTimeRange` reads. The
  /// override is useful with a custom boundary or a clock a test drives by
  /// hand. The standard channel boundary already supplies monotonic time.
  pub(crate) fn set_clock(&mut self, clock: Arc<dyn Clock>) {
    self.clock = Some(clock);
  }

  /// Returns the monotonic time that a finite restart window uses.
  fn now(&self) -> Duration {
    self
      .clock
      .as_ref()
      .map(|clock| clock.monotonic())
      .or_else(|| self.queue.boundary().monotonic_time())
      .expect("a finite restart window needs a monotonic clock")
  }

  fn require_restart_clock(&self, strategy: &SupervisorStrategy) {
    if let Some((limit, Some(_))) = strategy.restart_permission() {
      if !limit.forbids_restart() {
        self.now();
      }
    }
  }

  /// Installs the event stream the system publishes its own events to.
  ///
  /// Mirrors the stream `FaultHandling.publish` reaches through
  /// `context.system.eventStream`: a failure log the parent raises is published
  /// here, so a subscriber of the stream sees it. The executor keeps the stream
  /// apart from the sink because the sink is a test-side tap while the stream is
  /// the destination Pekko names.
  pub(crate) fn set_event_stream(&mut self, stream: EventStream) {
    self.event_stream = Some(stream);
  }

  pub(crate) fn event_sink(&self) -> Option<Arc<dyn EventSink>> {
    self.event_sink.lock().clone()
  }

  /// Installs the dispatcher and mailbox definitions that spawn-time
  /// resolution reads, from the system's
  /// [`DispatchSettings`](crate::kernel::actor::dispatch_settings::DispatchSettings) setup.
  pub(crate) fn set_dispatch_settings(&mut self, settings: DispatchSettings) {
    self.dispatch_settings = Some(settings);
  }

  /// Returns the (mailbox, stash) capacities resolved for `owner` when
  /// it was spawned, both unlimited when no definition applied.
  fn capacities_of(&self, owner: &ActorRef) -> (CapacityLimit, CapacityLimit) {
    self
      .actor_capacities
      .iter()
      .find(|(held, _, _)| held == owner)
      .map(|(_, mailbox, stash)| (*mailbox, *stash))
      .unwrap_or((CapacityLimit::Unlimited, CapacityLimit::Unlimited))
  }

  /// Publishes a [`DeadLetter`] for `message` on the event stream and
  /// hands the message to the dead letters handle, as Pekko's mailbox
  /// clean-up both publishes and delivers.
  fn publish_dead_letter(&mut self, message: Message, sender: ActorRef, recipient: &ActorRef) {
    // The dead letters handle publishes the wrapped event as it is, as
    // the mailbox clean-up paths of the dead letter port (#191) do.
    let wrapped = DeadLetter::from_message(message, sender.clone(), recipient.clone());
    let dead_letters = self.dead_letters();
    self.send(sender, &dead_letters, Message::new(wrapped));
  }

  /// Starts an actor that the system owns directly, below `/user`.
  pub(crate) fn spawn_user(&mut self, props: Props) -> ActorRef {
    self.assert_running();
    let name = alloc::format!("${}", next_spawn_name());
    let path = self.root.with_child("user").with_child(&name);
    self.spawn_at(props, path, None, SupervisorStrategy::default_strategy(), false)
  }

  /// Starts an actor below `parent`, using `name` when one is given.
  ///
  /// Mirrors `ActorContext.actorOf`: the child's path is the parent's path
  /// with the name appended, and a name drawn from a shared counter is used
  /// otherwise. The `strategy` is the caller's own choice and stays with the
  /// child, as `actorOf(props)` under an explicitly chosen strategy does.
  pub(crate) fn spawn_child(
    &mut self,
    parent: &ActorRef,
    props: Props,
    name: Option<&str>,
    strategy: SupervisorStrategy,
  ) -> ActorRef {
    assert!(
      !self.parents.iter().any(|entry| &entry.parent == parent && entry.stopping),
      "{}",
      InvalidActorNameException::terminating()
    );
    self.spawn_child_with_origin(parent, props, name, strategy, false)
  }

  /// Starts a child, recording whether its strategy is inherited from `parent`.
  fn spawn_child_with_origin(
    &mut self,
    parent: &ActorRef,
    props: Props,
    name: Option<&str>,
    strategy: SupervisorStrategy,
    strategy_inherited: bool,
  ) -> ActorRef {
    self.assert_running();
    self.require_restart_clock(&strategy);
    let label = match name {
      | Some(name) => name.to_string(),
      | None => alloc::format!("${}", next_spawn_name()),
    };
    let path = parent.path().with_child(&label);
    self.spawn_at(props, path, Some(parent.clone()), strategy, strategy_inherited)
  }

  fn spawn_at(
    &mut self,
    props: Props,
    path: ActorPath,
    parent: Option<ActorRef>,
    strategy: SupervisorStrategy,
    strategy_inherited: bool,
  ) -> ActorRef {
    self.assert_running();
    // Resolve the actor's capacities now, as Pekko resolves the
    // dispatcher and mailbox configuration when the actor is created:
    // a broken alias chain is a configuration error and panics, as the
    // original's `ConfigurationException` does, while an undefined name
    // keeps the unlimited behaviour (design of #192, H2).
    let (mailbox_capacity, stash_capacity) = match &self.dispatch_settings {
      | Some(settings) => match settings.resolve_capacities(props.dispatcher(), props.mailbox()) {
        | Ok(capacities) => capacities,
        | Err(error) => panic!("{error}"),
      },
      | None => (CapacityLimit::Unlimited, CapacityLimit::Unlimited),
    };
    let self_ref = next_actor_ref(path, self.queue.boundary().clone());
    // Only a bounded actor needs an entry: an actor whose capacities
    // both resolved to unlimited reads as unlimited without one, so
    // a system without bounded definitions does not grow the table
    // on every spawn (re-review 2, low 5 of #192). Moving the
    // capacities onto the cell itself waits for the per-actor
    // mailbox of #234, where the mailbox moves there too.
    if mailbox_capacity != CapacityLimit::Unlimited || stash_capacity != CapacityLimit::Unlimited {
      self.actor_capacities.push((self_ref.clone(), mailbox_capacity, stash_capacity));
    }
    let cell = ActorCell::pending(self_ref.clone(), parent, props, strategy, strategy_inherited);
    self.cells.push(cell);
    let index = self.cells.len() - 1;
    self.create_initial(index);
    self_ref
  }

  /// Records the strategy `parent` applies to the children it starts.
  ///
  /// Mirrors the `supervisorStrategy` a Pekko parent keeps for its `Children`.
  /// The executor stores it apart from the parent's cell because the cell is
  /// out of the table while the parent handles a message.
  pub(crate) fn register_parent_strategy(&mut self, parent: ActorRef, strategy: SupervisorStrategy) {
    self.require_restart_clock(&strategy);
    match self.parents.iter_mut().find(|entry| entry.parent == parent) {
      | Some(entry) => entry.strategy = strategy,
      | None => self.parents.push(ParentEntry { parent, strategy, children: Vec::new(), stopping: false }),
    }
  }

  /// Starts a named child of `parent` under the parent's strategy.
  ///
  /// Mirrors `ActorContext.actorOf(props, name)`. A name that a live child of
  /// `parent` already uses is refused with [`InvalidActorNameException`], and a
  /// name is reusable once that child has stopped. A parent that is terminating
  /// or terminated refuses every child. The name is checked as given; the port
  /// does not treat any prefix specially, so a caller that wants a generated
  /// name must generate one, as Pekko's `context.actorOf(props)` does.
  pub(crate) fn spawn_named_child(
    &mut self,
    parent: &ActorRef,
    props: Props,
    name: &str,
  ) -> Result<ActorRef, InvalidActorNameException> {
    let Some(strategy) = self.parents.iter().find(|entry| &entry.parent == parent).map(|entry| entry.strategy.clone())
    else {
      return Err(InvalidActorNameException::terminating());
    };
    self.spawn_named_child_under(parent, props, name, strategy, true)
  }

  /// Starts a named child of `parent` under `strategy`.
  ///
  /// Mirrors the named path of `TestProbe.childActorOf(props, name)`, which
  /// reaches `attachChild` without an actor context to report the refusal to.
  /// The name goes through the same registry as
  /// [`Executor::spawn_named_child`], so a name a live child uses is refused
  /// here too; a parent that has no registry yet gets one for its name
  /// bookkeeping. A parent that has already stopped keeps no registry, so a
  /// request against it is refused as terminating instead of reviving it.
  pub(crate) fn spawn_named_child_with_strategy(
    &mut self,
    parent: &ActorRef,
    props: Props,
    name: &str,
    strategy: SupervisorStrategy,
  ) -> Result<ActorRef, InvalidActorNameException> {
    if self.parents.iter().all(|entry| &entry.parent != parent) {
      if !self.is_live(parent) {
        return Err(InvalidActorNameException::terminating());
      }
      self.parents.push(ParentEntry {
        parent: parent.clone(),
        strategy: strategy.clone(),
        children: Vec::new(),
        stopping: false,
      });
    }
    self.spawn_named_child_under(parent, props, name, strategy, false)
  }

  /// Starts a named child once the checks shared by every named entry pass.
  ///
  /// The name is recorded only after the child has started. A `pre_start` that
  /// fails and makes the strategy stop the child removes its cell during
  /// creation, so the name is never taken by a child that is already gone and
  /// a later request may reuse it, as Pekko's `unreserveChild` allows.
  fn spawn_named_child_under(
    &mut self,
    parent: &ActorRef,
    props: Props,
    name: &str,
    strategy: SupervisorStrategy,
    strategy_inherited: bool,
  ) -> Result<ActorRef, InvalidActorNameException> {
    let Some(entry_index) = self.parents.iter().position(|entry| &entry.parent == parent) else {
      return Err(InvalidActorNameException::terminating());
    };
    if self.terminated || self.parents[entry_index].stopping {
      self.hook_child_creation_refused = true;
      return Err(InvalidActorNameException::terminating());
    }
    if self.parents[entry_index].children.iter().any(|(used, _)| used == name) {
      return Err(InvalidActorNameException::name_taken(name));
    }
    let label = name.to_string();
    let child = self.spawn_child_with_origin(parent, props, Some(&label), strategy, strategy_inherited);
    if self.cells.iter().any(|cell| cell.accepts(&child)) {
      if let Some(entry) = self.parents.iter_mut().find(|entry| &entry.parent == parent) {
        entry.children.push((label, child.clone()));
      }
    }
    Ok(child)
  }

  /// Starts a child for a caller that only holds a non-owning handle.
  ///
  /// Mirrors `TestProbe.childActorOf`: the caller has no way to receive a
  /// refusal, so a named request the registry rejects ends the call with the
  /// message Pekko throws from `attachChild`, and a request after the system
  /// has terminated still panics as [`Executor::spawn_child`] documents.
  /// An unnamed request reaches `attachChild` too, so a parent that is no
  /// longer live or whose registry is stopping refuses it with the same
  /// message instead of starting a child below a stopped actor.
  pub(crate) fn spawn_child_from_weak(
    &mut self,
    parent: &ActorRef,
    props: Props,
    name: Option<&str>,
    strategy: SupervisorStrategy,
  ) -> ActorRef {
    self.assert_running();
    match name {
      | Some(name) => match self.spawn_named_child_with_strategy(parent, props, name, strategy) {
        | Ok(child) => child,
        | Err(error) => panic!("{error}"),
      },
      | None => {
        let stopping = self.parents.iter().any(|entry| &entry.parent == parent && entry.stopping);
        assert!(self.is_live(parent) && !stopping, "{TERMINATED_MESSAGE}");
        self.spawn_child(parent, props, None, strategy)
      },
    }
  }

  /// Records a stop request to carry out after the actor that asked returns.
  pub(crate) fn request_stop(&mut self, target: ActorRef) {
    self.pending_stops.push(target);
  }

  /// Marks the start of one actor execution frame.
  fn enter_frame(&mut self, target: &ActorRef) {
    self.running.push(target.clone());
  }

  /// Marks the end of one actor execution frame.
  ///
  /// The frame is only popped here; the stops and child-terminated notices it
  /// left behind run once the outermost frame has returned, because the actor
  /// that asked for them is out of the cell table until then.
  fn exit_frame(&mut self) {
    self.running.pop();
  }

  /// Carries out the work deferred until the outermost actor frame returned.
  ///
  /// Pekko runs `context.stop` and the child-terminated hook after the actor
  /// that asked for them has returned to its cell. A nested frame, such as a
  /// child that an actor starts from `pre_start`, must not consume the requests
  /// of the frames around it, so only the outermost boundary drains them.
  fn finish_boundary(&mut self) {
    if !self.running.is_empty() || self.boundary_active {
      return;
    }
    self.boundary_active = true;
    loop {
      self.flush_ready_child_terminated();
      let applied = self.apply_pending_decider_failures();
      if self.pending_stops.is_empty() && !applied {
        break;
      }
      self.drain_pending_stops();
    }
    self.flush_ready_child_terminated();
    self.boundary_active = false;
  }

  /// Fails the actors whose decider panicked while they were still executing.
  ///
  /// The deciding actor is out of the cell table during its own delivery, so its
  /// failure waits for the outermost boundary, where the cell is back and the
  /// parent's strategy can decide for it. Returns whether a failure was applied,
  /// so the boundary repeats and carries out the stops or notices that applying
  /// it recorded.
  fn apply_pending_decider_failures(&mut self) -> bool {
    let pending = core::mem::take(&mut self.pending_decider_failures);
    for (supervisor, message) in &pending {
      self.apply_supervisor_failure(supervisor, message);
    }
    !pending.is_empty()
  }

  /// Carries out the stops that the actor asked for during its delivery.
  ///
  /// Pekko's `context.stop` is asynchronous, so the target is still alive for
  /// the rest of the delivery; the executor runs the stops only after the
  /// outermost frame has returned and the actor's cell is back in the table,
  /// which keeps the child alive while the actor still handles its message.
  fn drain_pending_stops(&mut self) {
    let stops = core::mem::take(&mut self.pending_stops);
    for target in stops {
      self.stop(&target);
    }
  }

  /// Carries out the pending stops aimed at the children of `parent` now.
  ///
  /// Mirrors the part of Pekko's `faultRecreate` that finishes recreating a
  /// parent only after the children it stopped in `preRestart` are gone: the
  /// replacement's `postRestart` runs after they have stopped. Only stops whose
  /// target is a live child of `parent` are taken here; any other pending stop
  /// belongs to a frame around this one and stays for `finish_boundary`, so a
  /// sibling's or an outer actor's stop is not consumed early. A child that is
  /// still running a frame of its own is left in place rather than stopped
  /// underneath it.
  fn complete_child_stops_of(&mut self, parent: &ActorRef) {
    let mut deferred = Vec::new();
    let mut ready = Vec::new();
    for target in core::mem::take(&mut self.pending_stops) {
      let is_child = self.cells.iter().any(|cell| cell.accepts(&target) && cell.is_child_of(parent));
      let is_running = self.running.iter().any(|running| running == &target);
      if is_child && !is_running {
        ready.push(target);
      } else {
        deferred.push(target);
      }
    }
    self.pending_stops = deferred;
    for target in ready {
      self.stop(&target);
    }
  }

  /// Runs a child-terminated notice whose parent is no longer executing.
  fn flush_ready_child_terminated(&mut self) {
    loop {
      let Some(index) = self
        .pending_child_terminated
        .iter()
        .position(|(parent, _)| !self.running.iter().any(|running| running == parent))
      else {
        return;
      };
      let (parent, child) = self.pending_child_terminated.remove(index);
      self.execute_child_terminated_hook(&parent, &child);
    }
  }

  /// Builds the actor of `index` and runs its `pre_start`, repeating while the
  /// strategy allows a restart on a failed startup and stopping once it does
  /// not.
  ///
  /// Mirrors Pekko's spawn path: a `preStart` that throws is wrapped in an
  /// `ActorInitializationException`, the parent decides whether to restart, and
  /// a child that stops because it never started runs no `postStop` because the
  /// actor was never created. Pekko's `faultCreate` stops the children the
  /// failed attempt left before it retries, so a child started by a failing
  /// `pre_start` does not keep its name into the next attempt.
  ///
  /// Stopping those children can run child-terminated hooks that stop other
  /// actors and remove their cells, so the index taken at the top of the loop
  /// may no longer address this actor after the stops. The cell is found again
  /// before creation is retried, as Pekko's `Children` keeps the same child
  /// across attempts; a cell that is gone leaves nothing to retry.
  fn create_initial(&mut self, mut index: usize) {
    let self_ref = self.cells[index].self_ref();
    loop {
      let catcher = self.catcher.clone();
      let mut cell = self.cells.remove(index);
      let mut stack = cell.take_stack();
      self.enter_frame(&self_ref);
      let outcome = {
        let mut context = ActorContext::new(self_ref.clone(), self_ref.clone(), self, &mut stack);
        match catcher {
          | Some(catcher) => catcher.run(&mut || cell.create(&mut context)),
          | None => {
            cell.create(&mut context);
            RunOutcome::Completed
          },
        }
      };
      self.exit_frame();
      match outcome {
        | RunOutcome::Completed => {
          cell.restore_stack(stack);
          self.cells.insert(index, cell);
          self.finish_boundary();
          return;
        },
        | RunOutcome::Failed { message } => {
          // Pekko's `failActor` clears the behaviour stack before a retry, so a
          // `become` issued by a `pre_start` that threw does not survive into
          // the next creation. Dropping `stack` leaves the fresh empty one that
          // `take_stack` put in its place.
          self.cells.insert(index, cell);
          self.publish_child_failure(&self_ref, ACTOR_INITIALIZATION_CAUSE, &message);
          let strategy = self.effective_strategy(&self.cells[index]);
          if strategy.is_all_for_one() {
            let retry = self.apply_all_for_one(
              &self_ref,
              &strategy,
              FailureKind::Initialization,
              ACTOR_INITIALIZATION_CAUSE,
              &message,
              None,
            );
            if !retry {
              self.finish_boundary();
              return;
            }
            self.stop_children_of(&self_ref);
            let Some(found) = self.cells.iter().position(|cell| cell.accepts(&self_ref)) else {
              self.finish_boundary();
              return;
            };
            index = found;
            continue;
          }
          let count = self.cells[index].restart_count();
          let decision = self.decide_catching(strategy, &self_ref, count, FailureKind::Initialization);
          match decision {
            | Ok(Directive::Restart) => {
              if !self.grant_restart_at(index) {
                self.stop(&self_ref);
                self.finish_boundary();
                return;
              }
              self.stop_children_of(&self_ref);
              let Some(found) = self.cells.iter().position(|cell| cell.accepts(&self_ref)) else {
                self.finish_boundary();
                return;
              };
              index = found;
            },
            | Ok(Directive::Stop) => {
              self.stop(&self_ref);
              self.finish_boundary();
              return;
            },
            | Err(message) => {
              let supervisor = self.cells[index].parent();
              self.fail_supervisor(supervisor, &self_ref, &message);
              self.finish_boundary();
              return;
            },
          }
        },
      }
    }
  }

  /// Stops every child of `parent`, letting `stop` carry out the usual order.
  pub(crate) fn stop_children_of(&mut self, parent: &ActorRef) {
    let children: Vec<ActorRef> =
      self.cells.iter().filter(|cell| cell.is_child_of(parent)).map(|cell| cell.self_ref()).collect();
    for child in children {
      self.stop(&child);
    }
  }

  /// Returns the strategy that decides for `cell`.
  ///
  /// A child started through `ActorContext::actorOf` inherits the strategy of
  /// the parent as it stands when the child fails, so a restart of the parent
  /// with a new `supervisor_strategy` also governs the children it already
  /// holds. A child that was started under an explicit strategy keeps it, and a
  /// child whose parent has no registry falls back to the strategy recorded
  /// when it was created.
  fn effective_strategy(&self, cell: &ActorCell) -> SupervisorStrategy {
    if cell.strategy_inherited() {
      if let Some(parent) = cell.parent() {
        if let Some(entry) = self.parents.iter().find(|entry| entry.parent == parent) {
          return entry.strategy.clone();
        }
      }
    }
    cell.strategy().clone()
  }

  /// Stops the actor or function reference `target`, telling every watcher that
  /// it is gone.
  ///
  /// Mirrors `org.apache.pekko.actor.ActorSystem.stop` together with
  /// `ActorContext.stop`: the children stop first, then the actor's `postStop`
  /// runs, then the cell is removed, and each registered watcher receives
  /// `Terminated`, and finally any reply the actor deferred is sent. Pekko stops
  /// the children before it finishes the parent's termination, so a child that
  /// is watched learns about its own end before the parent's watcher learns
  /// about the parent's.
  ///
  /// A function reference has no cell, so stopping one closes its boundary, which
  /// keeps its handler from running again, and removes it from the list that
  /// `watch` treats as live. Its children, if it was used as a parent, stop
  /// first under the same rule as a cell's children.
  ///
  /// The stopped actor keeps no entry in the parent registry, so a later named
  /// request against it is refused as terminating instead of reviving a stale
  /// registry.
  pub(crate) fn stop(&mut self, target: &ActorRef) {
    self.stop_with_sender(target, None);
  }

  /// Completes an external stop and drains requests made by its hooks.
  pub(crate) fn stop_at_boundary(&mut self, target: &ActorRef) {
    self.request_stop(target.clone());
    self.finish_boundary();
  }

  fn stop_with_sender(&mut self, target: &ActorRef, sender: Option<ActorRef>) {
    let is_cell = self.cells.iter().any(|cell| cell.accepts(target));
    let is_function_ref = self.function_refs.contains(target);
    if !is_cell && !is_function_ref {
      return;
    }
    self.mark_stopping(target);
    let parent_of_target = self.cells.iter().find(|cell| cell.accepts(target)).and_then(|cell| cell.parent());
    self.stop_children_of(target);
    self.stop_function_refs_of(target);
    if let Some(index) = self.function_refs.iter().position(|reference| reference == target) {
      let reference = self.function_refs.remove(index);
      reference.boundary().close();
    }
    if let Some(index) = self.cells.iter().position(|cell| cell.accepts(target)) {
      let mut cell = self.cells.remove(index);
      if !cell.post_stop_suppressed() {
        self.run_post_stop(&mut cell, sender);
      }
    }
    // A post_stop hook receives a context and can create function
    // references; those belong to the stopping actor and end here too.
    // References already ended above are gone from the registry, so
    // this second pass never notifies a watcher twice.
    self.stop_function_refs_of(target);
    self.forget_child(target);
    self.function_watching.retain(|(watcher, _)| watcher != target);
    self.stash_to_dead_letters(target);
    self.queued_to_dead_letters(target);
    self.watchers.retain(|(watcher, _)| watcher != target);
    self.pending_terminations.retain(|pending| &pending.watcher != target);
    self.notify_watchers(target, true, false);
    for callback in &self.termination_callbacks {
      callback(target);
    }
    self.flush_deferred_replies(target);
    self.actor_capacities.retain(|(held, _, _)| held != target);
    if let Some(parent) = parent_of_target {
      self.run_child_terminated_hook(&parent, target);
    }
    self.parents.retain(|entry| &entry.parent != target);
  }

  /// Marks `target` as stopping in the parent registry.
  fn mark_stopping(&mut self, target: &ActorRef) {
    if let Some(entry) = self.parents.iter_mut().find(|entry| &entry.parent == target) {
      entry.stopping = true;
    }
  }

  /// Forgets the names the stopped actor held as a child of its parent.
  fn forget_child(&mut self, child: &ActorRef) {
    for entry in &mut self.parents {
      entry.children.retain(|(_, held)| held != child);
    }
  }

  /// Runs `parent`'s child-terminated hook for the stopped `child`.
  ///
  /// Mirrors `FaultHandling.handleChildTerminated`: the parent's strategy hook
  /// runs with the parent's context after the child has fully stopped, so it
  /// may start a replacement child. A hook that tries to create a child while
  /// the parent is terminating is refused; the executor then logs the failure
  /// once as `IllegalStateException("handleChildTerminated failed")`, as Pekko
  /// does when the hook throws.
  ///
  /// A parent that is still executing is out of the cell table, so its notice
  /// waits until the outermost frame returns instead of being dropped.
  fn run_child_terminated_hook(&mut self, parent: &ActorRef, child: &ActorRef) {
    let Some(entry) = self.parents.iter().find(|entry| &entry.parent == parent) else {
      return;
    };
    if !entry.strategy.has_child_terminated() {
      return;
    }
    if self.running.iter().any(|running| running == parent) {
      self.pending_child_terminated.push((parent.clone(), child.clone()));
      return;
    }
    self.execute_child_terminated_hook(parent, child);
  }

  /// Runs `parent`'s child-terminated hook, restoring the parent cell whatever
  /// the hook does.
  ///
  /// A hook that throws is reported once as
  /// `IllegalStateException("handleChildTerminated failed")`, and the stop that
  /// triggered it still finishes. The hook does not run for a parent that never
  /// finished starting, matching Pekko's `if (actor != null)` guard.
  fn execute_child_terminated_hook(&mut self, parent: &ActorRef, child: &ActorRef) {
    let Some(entry) = self.parents.iter().find(|entry| &entry.parent == parent) else {
      return;
    };
    if !entry.strategy.has_child_terminated() {
      return;
    }
    let strategy = entry.strategy.clone();
    let siblings: Vec<ActorRef> =
      self.cells.iter().filter(|cell| cell.is_child_of(parent)).map(|cell| cell.self_ref()).collect();
    let Some(index) = self.cells.iter().position(|cell| cell.accepts(parent)) else {
      return;
    };
    if !self.cells[index].is_created() {
      return;
    }
    let mut cell = self.cells.remove(index);
    let mut stack = cell.take_stack();
    self.hook_child_creation_refused = false;
    self.enter_frame(parent);
    let catcher = self.catcher.clone();
    let outcome = {
      let mut context = ActorContext::new(parent.clone(), parent.clone(), self, &mut stack);
      match catcher {
        | Some(catcher) => catcher.run(&mut || strategy.handle_child_terminated(&mut context, child, &siblings)),
        | None => {
          strategy.handle_child_terminated(&mut context, child, &siblings);
          RunOutcome::Completed
        },
      }
    };
    self.exit_frame();
    cell.restore_stack(stack);
    self.cells.insert(index, cell);
    let refused = core::mem::replace(&mut self.hook_child_creation_refused, false);
    if matches!(outcome, RunOutcome::Failed { .. }) || refused {
      self.publish_child_failure(parent, HANDLE_CHILD_TERMINATED_FAILED_CAUSE, HANDLE_CHILD_TERMINATED_FAILED_MESSAGE);
    }
    self.finish_boundary();
  }

  /// Runs `postStop` on the behaviour of `index`, publishing an error when it
  /// throws.
  ///
  /// Mirrors `dungeon.FaultHandling`: a `postStop` that throws is logged as an
  /// error and the stop continues. A failure here never restarts the actor.
  /// The hook runs with the actor's context so a `postStop` that reads
  /// `sender()` sees the sender of the failing message when the actor is
  /// stopping because it failed a message, as Pekko's `postStop` does; every
  /// other stop gives the hook dead letters.
  fn run_post_stop(&mut self, cell: &mut ActorCell, sender: Option<ActorRef>) {
    let self_ref = cell.self_ref();
    let hook_sender = sender.unwrap_or_else(|| self.dead_letters());
    let mut stack = BehaviorStack::new();
    self.enter_frame(&self_ref);
    let catcher = self.catcher.clone();
    match catcher {
      | Some(catcher) => {
        let ran = {
          let mut context = ActorContext::new(self_ref.clone(), hook_sender, self, &mut stack);
          catcher.run(&mut || cell.post_stop_with_context(&mut context))
        };
        if let RunOutcome::Failed { message } = ran {
          self.publish_child_failure(&self_ref, EXCEPTION_CAUSE, &message);
        }
      },
      | None => {
        let mut context = ActorContext::new(self_ref.clone(), hook_sender, self, &mut stack);
        cell.post_stop_with_context(&mut context);
      },
    }
    self.exit_frame();
  }

  /// Registers a reply to `reply_to` after `target` stops.
  /// Returns a number that tells one delivery apart from every other.
  pub(crate) fn next_delivery_id(&mut self) -> u64 {
    self.next_delivery += 1;
    self.next_delivery
  }

  /// Puts `message` from `sender`, handled in `delivery`, aside in `owner`'s
  /// stash.
  ///
  /// # Panics
  ///
  /// Panics when the newest entry of the stash is the same delivery, as
  /// `UnrestrictedStash.stash` compares the current envelope with
  /// `theStash.last`.
  pub(crate) fn stash(&mut self, owner: ActorRef, delivery: u64, message: Message, sender: ActorRef) {
    if let Err(error) = self.try_stash(owner, delivery, message, sender) {
      panic!("{error}");
    }
  }

  /// Puts `message` aside as [`stash`](Self::stash) does, answering
  /// with [`StashOverflowException`] instead of panicking when the
  /// owner's stash is already at its resolved capacity; the state does
  /// not change in that case. The double-stash contract still panics,
  /// as it is a contract violation rather than a capacity one (design
  /// of #192, L4).
  pub(crate) fn try_stash(
    &mut self,
    owner: ActorRef,
    delivery: u64,
    message: Message,
    sender: ActorRef,
  ) -> Result<(), StashOverflowException> {
    if let Some((_, stash)) = self.stashes.iter().find(|(held, _)| held == &owner) {
      assert!(stash.last().is_none_or(|(last, _)| *last != delivery), "Can't stash the same message more than once");
    }
    let (_, stash_capacity) = self.capacities_of(&owner);
    let current = self.stashes.iter().find(|(held, _)| held == &owner).map(|(_, stash)| stash.len()).unwrap_or(0);
    if !stash_capacity.allows(current) {
      return Err(StashOverflowException::new(stash_capacity.bound().unwrap_or(0)));
    }
    let envelope = Envelope::new(owner.clone(), message, sender);
    match self.stashes.iter_mut().find(|(held, _)| held == &owner) {
      | Some((_, stash)) => stash.push((delivery, envelope)),
      | None => self.stashes.push((owner, alloc::vec![(delivery, envelope)])),
    }
    Ok(())
  }

  /// Sends `owner`'s stash to dead letters, each message with its original
  /// sender and in the order it was stashed.
  ///
  /// Mirrors what Pekko does with the stash of a stopped actor:
  /// `UnrestrictedStash.postStop` returns it to the mailbox, and the mailbox
  /// clean-up hands every message left there to `deadLetters`. Each message
  /// keeps its original recipient inside the `DeadLetter` it is wrapped in,
  /// so the dead letters handle publishes it as it is.
  fn stash_to_dead_letters(&mut self, owner: &ActorRef) {
    if let Some(index) = self.stashes.iter().position(|(held, _)| held == owner) {
      let (_, stash) = self.stashes.swap_remove(index);
      // Pekko's postStop returns the stash to the bounded mailbox: the
      // newest messages that fit stay in the mailbox and are cleaned up
      // in order, and the oldest excess goes to dead letters first,
      // newest victim first (design of #192, B4). An unlimited mailbox
      // keeps the previous behaviour of sending everything in order.
      let (mailbox_capacity, _) = self.capacities_of(owner);
      let queued = self.queue.count_addressed_to(owner);
      let bound = mailbox_capacity.bound().unwrap_or(usize::MAX).saturating_sub(queued);
      let total = stash.len();
      if total > bound {
        let excess = total - bound;
        let mut envelopes = stash.into_iter();
        let victims: Vec<_> = envelopes.by_ref().take(excess).collect();
        let rest: Vec<_> = envelopes.collect();
        for (_, envelope) in victims.into_iter().rev() {
          let (_, message, sender) = envelope.into_parts();
          self.publish_dead_letter(message, sender, owner);
        }
        for (_, envelope) in rest {
          let (_, message, sender) = envelope.into_parts();
          self.publish_dead_letter(message, sender, owner);
        }
      } else {
        for (_, envelope) in stash {
          let (_, message, sender) = envelope.into_parts();
          self.publish_dead_letter(message, sender, owner);
        }
      }
    }
  }

  /// Sends the envelopes still queued locally for the stopped `target` to dead
  /// letters, in queue order.
  ///
  /// Mirrors the mailbox clean-up after Pekko stops an actor: every message
  /// left in the mailbox, including a stash that `postStop` or a failed
  /// restart returned to it, goes to `deadLetters`. A queued DeathWatch notice
  /// goes as its `Terminated`. Envelopes for other actors stay where they are,
  /// and envelopes still waiting at the shard boundary reach the stopped actor
  /// later and are dropped there, as before.
  fn queued_to_dead_letters(&mut self, target: &ActorRef) {
    for envelope in self.queue.take_addressed_to(target) {
      let (_, message, sender) = envelope.into_parts();
      let message = match message.downcast_ref::<Arc<PendingTermination>>() {
        | Some(pending) => Message::new(pending.notice.clone()),
        | None => message,
      };
      self.publish_dead_letter(message, sender, target);
    }
  }

  /// Returns `owner`'s stash to the front of the local queue.
  ///
  /// A stashed `Terminated` is queued again as a pending DeathWatch notice, as
  /// Pekko's `UnrestrictedStash.enqueueFirst` calls `terminatedQueuedFor`: the
  /// actor re-receives it, and an `unwatch` of its target before it is handled
  /// cancels it, like any other queued notice. Pekko keys that record by the
  /// watched actor, so notices for one target share a single pending record:
  /// the first one handled consumes it and the others are dropped.
  pub(crate) fn unstash_all(&mut self, owner: &ActorRef) {
    if let Some(index) = self.stashes.iter().position(|(held, _)| held == owner) {
      let (_, stash) = self.stashes.swap_remove(index);
      // The bounded mailbox bounds this return, as Pekko's
      // `enqueueFirst` into a bounded deque does: the messages already
      // queued for the owner keep their room, the newest stashed
      // messages that fit are prepended, and the oldest excess goes to
      // dead letters, newest victim first (design of #192, B3). An
      // unlimited mailbox returns everything, as before.
      let (mailbox_capacity, _) = self.capacities_of(owner);
      let room = mailbox_capacity
        .bound()
        .map(|bound| bound.saturating_sub(self.queue.count_addressed_to(owner)))
        .unwrap_or(usize::MAX);
      let total = stash.len();
      let mut envelopes = stash.into_iter();
      if total > room {
        let victims: Vec<_> = envelopes.by_ref().take(total - room).collect();
        for (_, envelope) in victims.into_iter().rev() {
          let (_, message, sender) = envelope.into_parts();
          self.publish_dead_letter(message, sender, owner);
        }
      }
      let stash = envelopes.map(|(_, envelope)| self.requeue_notice(envelope)).collect();
      self.queue.prepend_locally(stash);
    }
  }

  fn requeue_notice(&mut self, envelope: Envelope) -> Envelope {
    let (recipient, message, sender) = envelope.into_parts();
    let message = match message.downcast_ref::<Terminated>() {
      | Some(notice) => {
        let queued = self.pending_terminations.iter().find(|pending| {
          pending.watcher == recipient && Self::same_watch_target(pending.notice.target(), notice.target())
        });
        let pending = match queued {
          | Some(pending) => pending.clone(),
          | None => {
            let pending = Arc::new(PendingTermination { watcher: recipient.clone(), notice: notice.clone() });
            self.pending_terminations.push(pending.clone());
            pending
          },
        };
        Message::new(pending)
      },
      | None => message,
    };
    Envelope::new(recipient, message, sender)
  }

  pub(crate) fn stop_and_reply(&mut self, target: &ActorRef, reply_to: ActorRef, message: Message) {
    self.deferred_replies.push((target.clone(), DeferredReply { reply_to, message }));
  }

  /// Sends the replies that `target` deferred when it stopped.
  fn flush_deferred_replies(&mut self, target: &ActorRef) {
    let mut remaining = Vec::new();
    let mut ready = Vec::new();
    for (actor, deferred) in self.deferred_replies.drain(..) {
      if &actor == target {
        ready.push(deferred);
      } else {
        remaining.push((actor, deferred));
      }
    }
    self.deferred_replies = remaining;
    for DeferredReply { reply_to, message } in ready {
      let sender = reply_to.clone();
      let envelope = Envelope::new(reply_to, message, sender);
      match envelope.into_local_if_same_shard(self.queue.boundary()) {
        | Ok(local) => self.queue.enqueue_locally(local),
        | Err(remote) => remote.post_to_boundary(),
      }
    }
  }

  /// Registers a function reference below `parent`, running `handler`
  /// for each message it receives.
  ///
  /// Mirrors `ActorCell.addFunctionRef`: the reference lives below the
  /// parent's path with a generated name, ends when the parent ends,
  /// and can be removed with [`remove_function_ref`](Self::remove_function_ref).
  pub(crate) fn add_function_ref<H>(&mut self, parent: &ActorRef, handler: H) -> FunctionRef
  where
    H: Fn(&ActorRef, Message) + Send + Sync + 'static,
  {
    self.assert_running();
    let name = alloc::format!("${}", next_spawn_name());
    let path = parent.path().with_child(&name);
    let dead_letters = Some(self.dead_letters());
    let boundary: Arc<dyn crate::kernel::executor::ShardBoundary> =
      Arc::new(FunctionBoundary::new_with_dead_letters(Arc::new(handler), dead_letters));
    let reference = next_actor_ref(path, boundary);
    self.function_refs.push(reference.clone());
    self.function_parents.push((reference.clone(), parent.clone()));
    FunctionRef::new(reference)
  }

  /// Removes the function reference `target` that `parent` created.
  ///
  /// Mirrors `ActorCell.removeFunctionRef`, whose `require` refuses a
  /// reference of another cell: removing another actor's function
  /// reference panics, as the original's `IllegalArgumentException`
  /// fails the actor. Removing one's own reference is idempotent, and
  /// the removal tells its watchers and closes its boundary, so later
  /// messages go to dead letters.
  pub(crate) fn remove_function_ref(&mut self, parent: &ActorRef, target: &FunctionRef) {
    let target = target.as_actor_ref();
    // The original's `require(ref.path.parent eq self.path)` compares
    // paths, so the refusal does not depend on the registry: a
    // reference that was already removed still refuses a removal by
    // another cell (re-review 2, low 4 of #192).
    assert!(
      target.path().parent() == parent.path(),
      "removeFunctionRef: {} is not a function ref of {}",
      target.path(),
      parent.path()
    );
    if self.function_refs.contains(&target) {
      self.stop_function_ref(&target);
    }
  }

  /// Ends one live function reference: closes its boundary, clears the
  /// watches it held, and tells its watchers, as a stop does.
  fn stop_function_ref(&mut self, target: &ActorRef) {
    if let Some(index) = self.function_refs.iter().position(|reference| reference == target) {
      let reference = self.function_refs.remove(index);
      reference.boundary().close();
    }
    self.function_parents.retain(|(held, _)| held != target);
    self.function_watching.retain(|(watcher, _)| watcher != target);
    self.watchers.retain(|(watcher, _)| watcher != target);
    self.pending_terminations.retain(|pending| &pending.watcher != target);
    self.notify_watchers(target, true, false);
    for callback in &self.termination_callbacks {
      callback(target);
    }
  }

  /// Ends every function reference `parent` created, as the parent's
  /// own stop ends them in Pekko.
  fn stop_function_refs_of(&mut self, parent: &ActorRef) {
    let owned: Vec<ActorRef> = self
      .function_parents
      .iter()
      .filter(|(_, owner)| owner == parent)
      .map(|(reference, _)| reference.clone())
      .filter(|reference| self.function_refs.contains(reference))
      .collect();
    for reference in owned {
      self.stop_function_ref(&reference);
    }
  }

  /// Registers the function reference `watcher` to receive the
  /// `Terminated` of `target` as a message.
  ///
  /// Mirrors `FunctionRef.watch`: the registration is kept apart from
  /// the delivery watchers table, because it survives the notice until
  /// the reference unwatches or stops (design of #192, F3). A target
  /// that is not live is reported at once, as `watch` does.
  pub(crate) fn function_ref_watch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef {
    let watcher = watcher.as_actor_ref();
    if target.is_detached() || self.is_live(target) {
      if !self
        .function_watching
        .iter()
        .any(|(held, watched)| held == &watcher && Self::same_watch_target(watched, target))
      {
        self.function_watching.push((watcher, target.clone()));
      }
    } else {
      let notice = Terminated::new(target.clone(), false, false);
      Envelope::new(watcher, Message::new(notice), target.clone()).post_to_boundary();
    }
    target.clone()
  }

  /// Stops the function reference `watcher` from watching `target`.
  ///
  /// Mirrors `FunctionRef.unwatch`, which is idempotent.
  pub(crate) fn function_ref_unwatch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef {
    let watcher = watcher.as_actor_ref();
    self.function_watching.retain(|(held, watched)| held != &watcher || !Self::same_watch_target(watched, target));
    target.clone()
  }

  /// Returns whether the function reference `watcher` watches `target`.
  ///
  /// Mirrors `FunctionRef.isWatching`: the registration survives the
  /// target's end, so this stays true after the notice until
  /// [`function_ref_unwatch`](Self::function_ref_unwatch) or the
  /// reference's own stop.
  pub(crate) fn function_ref_is_watching(&self, watcher: &FunctionRef, target: &ActorRef) -> bool {
    let watcher = watcher.as_actor_ref();
    self.function_watching.iter().any(|(held, watched)| held == &watcher && Self::same_watch_target(watched, target))
  }

  /// Registers `watcher` to receive the `Terminated` notice of `target`.
  ///
  /// Mirrors `ActorContext.watch` for the scope this port can serve: actors of
  /// this system, and the function references that this system made for its
  /// test actors. A live target of either kind is registered as a watcher; the
  /// notice comes when it is stopped.
  ///
  /// Detached targets are registered but never send a watch response. Their
  /// watch is distinct from a watch of the original delivery reference.
  ///
  /// Anything else is outside that scope: an actor of another system, a
  /// reference that has already stopped, and the temporary reply address that
  /// an `ask` creates. Those are reported at once with `existence_confirmed =
  /// false` and `address_terminated = false`, and no later notice follows. A
  /// target that keeps running out of scope is therefore indistinguishable from
  /// a stopped one, which is the documented limit of this port.
  pub(crate) fn watch(&mut self, watcher: &ActorRef, target: &ActorRef) {
    if target.is_detached() || self.is_live(target) {
      if !self
        .watchers
        .iter()
        .any(|(existing, watched)| existing == watcher && Self::same_watch_target(watched, target))
      {
        self.watchers.push((watcher.clone(), target.clone()));
      }
    } else {
      self.enqueue_to(watcher.clone(), Terminated::new(target.clone(), false, false));
    }
  }

  /// Stops `watcher` from receiving the notice of `target`.
  ///
  /// Mirrors `ActorContext.unwatch`, which Pekko's event stream unsubscriber
  /// calls once a subscriber holds no subscription. Removing a watch that is
  /// not registered changes nothing. A local notice still queued for the
  /// removed watch is also cancelled.
  pub(crate) fn unwatch(&mut self, watcher: &ActorRef, target: &ActorRef) {
    self.watchers.retain(|(existing, watched)| existing != watcher || !Self::same_watch_target(watched, target));
    self
      .pending_terminations
      .retain(|pending| &pending.watcher != watcher || !Self::same_watch_target(pending.notice.target(), target));
  }

  /// Delivers every message that is waiting, whether it is in the shard or at
  /// its boundary, until nothing is left.
  pub(crate) fn run_until_idle(&mut self) {
    while self.run_one() {}
  }

  /// Delivers at most one message: it takes at most one envelope from the
  /// boundary onto the local queue, then delivers at most one envelope from the
  /// local queue. Returns whether it delivered one.
  ///
  /// Taking only one envelope per step keeps a boundary that never runs dry
  /// from starving the local queue. The caller of `run_until_idle` still sees
  /// every message, because the loop ends only once both the boundary and the
  /// local queue are empty.
  pub(crate) fn run_one(&mut self) -> bool {
    self.queue.take_one_from_boundary();
    match self.queue.pop() {
      | Some(envelope) => {
        self.deliver(envelope);
        true
      },
      | None => false,
    }
  }

  /// Waits at most `max` for one envelope from the boundary and moves it onto
  /// the local queue. Returns whether it found one.
  pub(crate) fn await_envelope(&mut self, max: Duration) -> bool {
    self.queue.await_one_from_boundary(max)
  }

  /// Drops the actors and every queued message.
  ///
  /// The boundary is closed first, so an `ActorRef` that is still around cannot
  /// put a message back into it after it has been drained; otherwise the queued
  /// envelope would keep the boundary alive through the handle's `Arc` and the
  /// system would never be released.
  ///
  /// The function references are closed in the same step. They carry no cell,
  /// so closing the shard boundary alone would leave a test actor running a
  /// handler for every message that reaches its handle after the terminate,
  /// where Pekko's test actor has stopped and its messages are dead letters.
  pub(crate) fn stop_all(&mut self) {
    self.terminated = true;
    self.queue.boundary().close();
    for reference in &self.function_refs {
      reference.boundary().close();
    }
    self.function_refs.clear();
    self.function_parents.clear();
    self.function_watching.clear();
    self.cells.clear();
    self.queue.clear();
    self.watchers.clear();
    self.pending_terminations.clear();
    self.stashes.clear();
    self.deferred_replies.clear();
    self.parents.clear();
    self.pending_stops.clear();
    self.pending_decider_failures.clear();
    self.pending_child_terminated.clear();
    self.running.clear();
    self.termination_callbacks.clear();
    self.boundary_active = false;
    while self.queue.boundary().poll().is_some() {}
  }

  /// Returns whether the system has been terminated.
  pub(crate) fn is_terminated(&self) -> bool {
    self.terminated
  }

  /// Returns whether `target` no longer has a running actor behind it.
  ///
  /// Mirrors `ActorRef.isTerminated`: a handle reports false while its actor
  /// runs and true once the actor has stopped. A handle to a restarted actor
  /// keeps the same identifier and generation and the restarted cell answers
  /// for it, so the handle reports false again after the restart. A handle of
  /// another system, or the temporary reply address of an `ask`, has no cell
  /// in this shard and reports true, the same limit `watch` documents.
  pub(crate) fn is_actor_terminated(&self, target: &ActorRef) -> bool {
    !self.is_live(target)
  }

  pub(crate) fn stopped_noop_ref(&self, old: &ActorRef) -> ActorRef {
    assert!(
      old.shares_boundary_with(self.queue.boundary()) && self.is_actor_terminated(old),
      "expected a stopped actor on this shard: {}",
      old.path()
    );
    old.detached_noop()
  }

  /// Returns the identity of the shard boundary this executor posts to.
  pub(crate) fn boundary_key(&self) -> usize {
    Arc::as_ptr(self.queue.boundary()) as *const () as usize
  }

  /// Refuses to start an actor once the system has been terminated.
  fn assert_running(&self) {
    assert!(!self.terminated, "{TERMINATED_MESSAGE}");
  }

  /// Returns whether `target` can still be watched for its end.
  ///
  /// Delivers the message a selection envelope carries, resolving its
  /// path now.
  ///
  /// Mirrors `ActorSelection.deliverSelection`: a path that names a live
  /// actor or a function reference of this shard is delivered to it; any
  /// other path is routed by the dead letter rules with an empty reference
  /// for that path as the recipient, and a suppressed payload names that
  /// empty reference as the reference that processed it.
  fn deliver_selection(&mut self, path: &ActorPath, message: Message, sender: ActorRef) {
    if let Some(cell) = self.cells.iter().find(|cell| &cell.self_ref().path() == path) {
      let target = cell.self_ref();
      self.deliver(Envelope::new(target, message, sender));
      return;
    }
    if let Some(reference) = self.function_refs.iter().find(|reference| &reference.path() == path) {
      let target = reference.clone();
      self.send(sender, &target, message);
      return;
    }
    let empty = next_actor_ref(path.clone(), self.queue.boundary().clone());
    self.dead_letters_shared.route_unresolved(message, sender, empty);
  }

  /// The cell table does not hold the actor that is currently executing it, so
  /// a frame that is running counts as live as well; otherwise an actor would
  /// see itself as terminated from inside `receive`, `pre_start`,
  /// `post_restart`, or a child-terminated hook.
  fn is_live(&self, target: &ActorRef) -> bool {
    self.cells.iter().any(|cell| cell.accepts(target))
      || self.function_refs.contains(target)
      || self.running.iter().any(|running| running == target)
  }

  /// Sends `message` to `target`, naming `sender` as the sender of the
  /// envelope.
  ///
  /// A message to an actor of this shard goes onto the local queue; a message
  /// to an actor of another shard, or to a function reference, goes through the
  /// recipient's boundary.
  pub(crate) fn send(&mut self, sender: ActorRef, target: &ActorRef, message: Message) {
    let envelope = Envelope::new(target.clone(), message, sender);
    match envelope.into_local_if_same_shard(self.queue.boundary()) {
      | Ok(local) => self.queue.enqueue_locally(local),
      | Err(remote) => remote.post_to_boundary(),
    }
  }

  /// Delivers one envelope.
  ///
  /// The envelope carries three kinds of message: `Kill` and `PoisonPill`, which
  /// act on the actor without reaching `receive`, and any other message, which
  /// the actor handles.
  ///
  /// The cell is taken out of the table before the actor runs, so the actor may
  /// start a child or stop itself through the context without borrowing the
  /// table twice. The context asks the executor directly, and the executor
  /// carries out a stop-after-reply once the actor returns.
  fn deliver(&mut self, envelope: Envelope) {
    let (recipient, message, sender) = envelope.into_parts();
    if message.downcast_ref::<StopDeadLetterListener>().is_some() {
      // The dead letter listener's handler posted this notice when its
      // state machine stopped itself. Carry the stop out here, where the
      // executor can be borrowed, and run the boundary work so this same
      // pass drains the stop.
      if let Some(listener) = self.dead_letter_listener.take() {
        self.request_stop(listener);
      }
      self.finish_boundary();
      return;
    }
    if let Some(selection) = message.downcast_ref::<ActorSelectionMessage>() {
      let selection = selection.clone();
      self.deliver_selection(selection.path(), selection.message().clone(), selection.sender().clone());
      self.finish_boundary();
      return;
    }
    let message = if let Some(pending) = message.downcast_ref::<Arc<PendingTermination>>() {
      let Some(index) = self.pending_terminations.iter().position(|queued| Arc::ptr_eq(queued, pending)) else {
        return;
      };
      self.pending_terminations.swap_remove(index);
      Message::new(pending.notice.clone())
    } else {
      message
    };
    let Some(index) = self.cells.iter().position(|cell| cell.accepts(&recipient)) else {
      // No actor answers for the recipient: the message is routed by the
      // dead letter rules, as a Pekko mailbox hands a message for a
      // stopped actor to the dead letters provider.
      let dead_letters = self.dead_letters.clone();
      self.dead_letters_shared.route_undeliverable(message, sender, recipient, &dead_letters);
      // The routing publishes synchronously, so the dead letter listener
      // may have reached its limit inside this delivery; the boundary
      // work (including the listener's own stop) runs here as it does at
      // the end of a delivered message.
      self.finish_boundary();
      return;
    };
    if message == KILL {
      self.handle_kill(index, &recipient);
      return;
    }
    if message == POISON_PILL {
      self.stop_at_boundary(&recipient);
      return;
    }
    let mut cell = self.cells.remove(index);
    let mut stack = cell.take_stack();
    let delivered = message.clone();
    let catcher = self.catcher.clone();
    self.enter_frame(&recipient);
    let outcome = {
      let mut context = ActorContext::new(recipient.clone(), sender.clone(), self, &mut stack);
      match catcher {
        | Some(catcher) => catcher.run(&mut || cell.deliver(&mut context, delivered.clone())),
        | None => {
          cell.deliver(&mut context, delivered);
          RunOutcome::Completed
        },
      }
    };
    self.exit_frame();
    cell.restore_stack(stack);
    match outcome {
      | RunOutcome::Completed => {
        self.cells.push(cell);
        if self.deferred_stop(&recipient) {
          self.stop(&recipient);
        }
      },
      | RunOutcome::Failed { message } => {
        if self.deferred_stop(&recipient) {
          self.cells.push(cell);
          self.stop_with_sender(&recipient, Some(sender));
        } else {
          self.apply_failure(cell, &recipient, RUNTIME_CAUSE, &message, Some(sender));
        }
      },
    }
    self.finish_boundary();
  }

  /// Returns whether the actor asked to stop after the reply it registered.
  fn deferred_stop(&self, target: &ActorRef) -> bool {
    self.deferred_replies.iter().any(|(actor, _)| actor == target)
  }

  /// Applies the supervisor's decision to a failure during `receive`, then
  /// carries out the requests the actor recorded before it failed.
  ///
  /// `original_sender` is the sender of the message whose handling failed, and
  /// is handed to the failed child's lifecycle hooks so a hook that reads
  /// `sender()` sees it. Only the message-handling path supplies it; every
  /// other failure entry passes `None`.
  fn apply_failure(
    &mut self,
    mut cell: ActorCell,
    target: &ActorRef,
    cause_name: &str,
    message: &str,
    original_sender: Option<ActorRef>,
  ) {
    self.publish_child_failure(target, cause_name, message);
    let strategy = self.effective_strategy(&cell);
    if strategy.is_all_for_one() {
      self.cells.push(cell);
      self.apply_all_for_one(target, &strategy, FailureKind::MessageHandling, cause_name, message, original_sender);
      return;
    }
    let count = cell.restart_count();
    let supervisor = cell.parent();
    let decision = self.decide_catching(strategy, target, count, FailureKind::MessageHandling);
    match decision {
      | Ok(Directive::Restart) => {
        if self.grant_restart(&mut cell) {
          self.restart_cell(target, cell, FailureKind::MessageHandling, cause_name, message, original_sender);
        } else {
          self.cells.push(cell);
          self.stop_with_sender(target, original_sender);
        }
      },
      | Ok(Directive::Stop) => {
        self.cells.push(cell);
        self.stop_with_sender(target, original_sender);
      },
      | Err(message) => {
        self.cells.push(cell);
        self.fail_supervisor(supervisor, target, &message);
      },
    }
  }

  /// Records a restart request on `cell` after the strategy chose `Restart`,
  /// and answers whether that restart is permitted.
  ///
  /// Mirrors `OneForOneStrategy.processFailure`, which restarts only when
  /// `stats.requestRestartPermission(retriesWindow)` grants it and stops the
  /// child otherwise. A strategy with a finite window answers through the
  /// child's [`RestartHistory`](crate::kernel::supervision) at `now`, which
  /// both counts the request and opens or reopens the window in one step; the
  /// plain cumulative record must not be added on top or the request would
  /// count twice. A strategy without a finite window keeps the existing
  /// contract: the directive already applied any cumulative limit. An
  /// unlimited one-for-one strategy does not record the request.
  fn grant_restart(&mut self, cell: &mut ActorCell) -> bool {
    let strategy = self.effective_strategy(cell);
    match strategy.restart_permission() {
      | Some((limit, window)) => self.apply_permission(cell, limit, window),
      | None => {
        if strategy.records_plain_restart() {
          cell.record_restart_request();
        }
        true
      },
    }
  }

  /// Records a restart request on the cell at `index` and answers whether the
  /// restart is permitted, as [`Executor::grant_restart`] does for a taken cell.
  fn grant_restart_at(&mut self, index: usize) -> bool {
    let strategy = self.effective_strategy(&self.cells[index]);
    match strategy.restart_permission() {
      | Some((limit, window)) => {
        let mut history = self.cells[index].restart_history();
        let allowed = self.apply_permission_to(&mut history, limit, window);
        self.cells[index].set_restart_history(history);
        allowed
      },
      | None => {
        if strategy.records_plain_restart() {
          self.cells[index].record_restart_request();
        }
        true
      },
    }
  }

  /// Answers whether a restart is permitted for `cell` under `limit` and
  /// `window`, updating its history, without the plain cumulative record.
  fn apply_permission(&mut self, cell: &mut ActorCell, limit: RestartLimit, window: Option<Duration>) -> bool {
    let mut history = cell.restart_history();
    let allowed = self.apply_permission_to(&mut history, limit, window);
    cell.set_restart_history(history);
    allowed
  }

  /// Reads the monotonic clock only when `limit` and `window` need it, then asks
  /// `history` to permit the restart, as `AllForOneStrategy.processFailure` does.
  ///
  /// A forbidding limit denies without reading the clock, so a finite window over
  /// a zero limit keeps the custom-boundary contract that supplies no clock.
  fn apply_permission_to(&self, history: &mut RestartHistory, limit: RestartLimit, window: Option<Duration>) -> bool {
    let now = if window.is_some() && !limit.forbids_restart() { self.now() } else { Duration::ZERO };
    history.request_permission(limit, window, now)
  }

  /// Applies an all-for-one decision: restart every direct sibling when all of
  /// them may still restart, or stop every one otherwise.
  ///
  /// Mirrors `AllForOneStrategy.processFailure`: the failed child's failure is
  /// judged for the whole sibling set. When the failed child's directive is
  /// `Restart` and every sibling has a restart left in the retry count and time
  /// window, each sibling restarts; the failed child keeps the message's
  /// original sender so its hooks see it, and every other sibling gets `None`.
  /// When any sibling has no restart left, or the failed child's directive is
  /// not `Restart`, every sibling stops. The failed cell must already be back
  /// in the table when this runs.
  /// Returns whether a failed initial creation may retry in its caller's loop;
  /// a cell that has not been created has no old instance to restart.
  fn apply_all_for_one(
    &mut self,
    target: &ActorRef,
    strategy: &SupervisorStrategy,
    failure: FailureKind,
    cause_name: &str,
    message: &str,
    original_sender: Option<ActorRef>,
  ) -> bool {
    let Some(parent) = self.cells.iter().find(|cell| cell.accepts(target)).and_then(ActorCell::parent) else {
      return false;
    };
    let siblings: Vec<ActorRef> =
      self.cells.iter().filter(|cell| cell.is_child_of(&parent)).map(ActorCell::self_ref).collect();
    let restart = match self.decide_catching(strategy.clone(), target, 0, failure) {
      | Ok(directive) => directive == Directive::Restart,
      | Err(message) => {
        self.fail_supervisor(Some(parent), target, &message);
        return false;
      },
    };
    let mut permitted = restart;
    if restart {
      if let Some((limit, window)) = strategy.restart_permission() {
        let now = if window.is_some() && !limit.forbids_restart() { self.now() } else { Duration::ZERO };
        for sibling in &siblings {
          let Some(index) = self.cells.iter().position(|cell| cell.accepts(sibling)) else {
            permitted = false;
            break;
          };
          let mut history = self.cells[index].restart_history();
          let allowed = history.request_permission(limit, window, now);
          self.cells[index].set_restart_history(history);
          if !allowed {
            permitted = false;
            break;
          }
        }
      }
    }
    // Hooks can request another sibling's stop. Drain those requests only
    // after every sibling has received this supervision decision.
    let was_active = core::mem::replace(&mut self.boundary_active, true);
    if permitted {
      for sibling in siblings {
        if &sibling == target && failure == FailureKind::Initialization {
          continue;
        }
        let Some(index) = self.cells.iter().position(|cell| cell.accepts(&sibling)) else {
          continue;
        };
        let cell = self.cells.remove(index);
        let sender = if &sibling == target { original_sender.clone() } else { None };
        self.restart_cell(&sibling, cell, failure, cause_name, message, sender);
      }
    } else {
      for sibling in siblings {
        let sender = if &sibling == target { original_sender.clone() } else { None };
        self.stop_with_sender(&sibling, sender);
      }
    }
    self.boundary_active = was_active;
    permitted && failure == FailureKind::Initialization
  }

  /// Asks `strategy` for the directive for `child`, catching a decider panic.
  ///
  /// The decider runs outside the work the caller already wrapped, so a panic
  /// there would unwind past the cell the caller had taken out of the table.
  /// Catching it here returns the panic message so the caller can put its cell
  /// back and fail the deciding actor in its turn, as Pekko's `systemInvoke`
  /// does when the failure handling throws. A system without a catcher keeps
  /// the existing contract: the panic unwinds as any other actor panic does.
  fn decide_catching(
    &self,
    strategy: SupervisorStrategy,
    child: &ActorRef,
    restart_count: u32,
    failure: FailureKind,
  ) -> Result<Directive, String> {
    let Some(catcher) = self.catcher.clone() else {
      return Ok(strategy.decide_for_child(child, restart_count, failure));
    };
    let mut directive = None;
    let outcome = catcher.run(&mut || {
      directive = Some(strategy.decide_for_child(child, restart_count, failure));
    });
    match outcome {
      | RunOutcome::Completed => Ok(directive.expect("a completed decider answered for its child")),
      | RunOutcome::Failed { message } => Err(message),
    }
  }

  /// Fails the actor that decided for a child when its decider panicked.
  ///
  /// The panic is the failure of the deciding actor, not of the child, so the
  /// deciding actor goes through the same path as a panic in `receive` and its
  /// own parent chooses `Restart` or `Stop`. A deciding actor that is still
  /// executing has no cell in the table, so its failure waits for the outermost
  /// boundary instead of being dropped or applied to the child. A deciding actor
  /// without a cell, such as a function reference, cannot fail in its turn, so
  /// the panic is reported once and the restored child stays as it is.
  fn fail_supervisor(&mut self, supervisor: Option<ActorRef>, failed: &ActorRef, message: &str) {
    let Some(supervisor) = supervisor else {
      self.publish_child_failure(failed, RUNTIME_CAUSE, message);
      return;
    };
    if self.running.iter().any(|running| running == &supervisor) {
      self.pending_decider_failures.push((supervisor, message.to_string()));
      return;
    }
    self.apply_supervisor_failure(&supervisor, message);
  }

  /// Applies the failure of `supervisor` under the strategy of its own parent.
  ///
  /// The deciding actor failed while handling a message, so a `MessageHandling`
  /// failure reaches `apply_failure` on the same path a panic in `receive` uses.
  /// The panic is reported here, once; the capture that saw it does not report
  /// it again. A deciding actor that has stopped since the panic was captured
  /// has no cell to fail, so the panic is reported once and nothing else runs.
  fn apply_supervisor_failure(&mut self, supervisor: &ActorRef, message: &str) {
    let Some(index) = self.cells.iter().position(|cell| cell.accepts(supervisor)) else {
      self.publish_child_failure(supervisor, RUNTIME_CAUSE, message);
      return;
    };
    let cell = self.cells.remove(index);
    self.apply_failure(cell, supervisor, RUNTIME_CAUSE, message, None);
  }

  /// Re-creates the actor of `cell` and runs its `postRestart`, stopping it when
  /// the recipe or `postRestart` fails.
  ///
  /// Mirrors Pekko's restart path: `preRestart` runs on the old instance, the
  /// recipe creates the new one, and `postRestart` runs on it. A failure of the
  /// recipe or of `postRestart` stops the actor, as the default decider maps
  /// `PostRestartException` to stop. The stop path then skips the old
  /// instance's `post_stop`, because `preRestart` already ran it and the
  /// instance must not run it twice.
  ///
  /// `original_sender` is the sender of the message whose handling failed, and
  /// becomes the `sender()` the old instance's `pre_restart` sees, as Pekko's
  /// `preRestart` sees the sender of the failing message. When it is `None`,
  /// as for a `Kill` or a supervisor failure, the hook sees the actor's own
  /// handle.
  fn restart_cell(
    &mut self,
    target: &ActorRef,
    mut cell: ActorCell,
    failure: FailureKind,
    cause_name: &str,
    message: &str,
    original_sender: Option<ActorRef>,
  ) {
    // Pekko's `UnrestrictedStash.preRestart` returns the stash to the mailbox
    // before the old instance's `preRestart`, so the new instance handles it.
    self.unstash_all(target);
    let reason = self.failure_reason(target, cause_name, message);
    let reason_box: Box<dyn core::any::Any + Send> = Box::new(reason);
    let catcher = self.catcher.clone();
    let mut old_stack = cell.take_stack();
    self.enter_frame(target);
    let hook_sender = original_sender.unwrap_or_else(|| target.clone());
    let pre_restart_failed = {
      let mut context = ActorContext::new(target.clone(), hook_sender, self, &mut old_stack);
      match catcher {
        | Some(catcher) => catcher.run(&mut || cell.pre_restart(&mut context, reason_box.as_ref())),
        | None => {
          cell.pre_restart(&mut context, reason_box.as_ref());
          RunOutcome::Completed
        },
      }
    };
    if let RunOutcome::Failed { message } = pre_restart_failed {
      self.publish_child_failure(target, RUNTIME_CAUSE, &message);
    }
    // Pekko's `faultRecreate` stops the actor's children and runs the new
    // instance's `postRestart` only after they have finished stopping. The
    // port records `context.stop` from `pre_restart` as a pending stop; carry
    // out the ones aimed at this actor's own children now, before the
    // replacement is created, so `post_restart` already sees them stopped.
    // Stops asked for by frames around this one are left to `finish_boundary`.
    self.complete_child_stops_of(target);
    self.exit_frame();
    // Pekko's finishRecreate snapshots surviving children before the new
    // instance can spawn its own. Apply that propagation to a supervised Kill.
    let survivors = if failure == FailureKind::Killed {
      self
        .cells
        .iter()
        .filter(|child| child.is_child_of(target) && child.is_created())
        .map(ActorCell::self_ref)
        .collect::<Vec<_>>()
    } else {
      Vec::new()
    };
    let catcher = self.catcher.clone();
    let mut stack = BehaviorStack::new();
    self.enter_frame(target);
    let restarted = {
      let mut context = ActorContext::new(target.clone(), target.clone(), self, &mut stack);
      match catcher {
        | Some(catcher) => catcher.run(&mut || cell.restart(&mut context)),
        | None => {
          cell.restart(&mut context);
          RunOutcome::Completed
        },
      }
    };
    self.exit_frame();
    cell.restore_stack(stack);
    match restarted {
      | RunOutcome::Completed => {
        self.cells.push(cell);
        // A retained child's decider can fail again; defer that failure until
        // this recovery returns instead of restarting the same parent inside it.
        self.enter_frame(target);
        for child in survivors {
          if self.pending_stops.contains(target) || self.pending_stops.contains(&child) {
            continue;
          }
          if let Some(index) = self.cells.iter().position(|cell| cell.accepts(&child)) {
            let cell = self.cells.remove(index);
            self.restart_cell(&child, cell, failure, cause_name, message, None);
          }
        }
        let children: Vec<ActorRef> = self
          .cells
          .iter()
          .filter(|child| child.is_child_of(target) && !child.is_created())
          .map(|child| child.self_ref())
          .collect();
        for child in children {
          if !self.cells.iter().any(|cell| cell.accepts(&child) && !cell.is_created()) {
            continue;
          }
          self.stop_children_of(&child);
          if let Some(index) = self.cells.iter().position(|cell| cell.accepts(&child) && !cell.is_created()) {
            self.create_initial(index);
          }
        }
        self.exit_frame();
      },
      | RunOutcome::Failed { message } => {
        self.publish_child_failure(target, RUNTIME_CAUSE, &message);
        cell.suppress_post_stop();
        self.cells.push(cell);
        self.stop(target);
      },
    }
    self.finish_boundary();
  }

  /// Handles a `Kill`: the instance fails with `ActorKilledException` and the
  /// supervisor decides for a killed child, which the default decider maps to
  /// `Stop`.
  fn handle_kill(&mut self, index: usize, recipient: &ActorRef) {
    self.publish_child_failure(recipient, ACTOR_KILLED_CAUSE, ACTOR_KILLED_MESSAGE);
    let mut cell = self.cells.remove(index);
    let strategy = self.effective_strategy(&cell);
    if strategy.is_all_for_one() {
      self.cells.push(cell);
      self.apply_all_for_one(recipient, &strategy, FailureKind::Killed, ACTOR_KILLED_CAUSE, ACTOR_KILLED_MESSAGE, None);
      self.finish_boundary();
      return;
    }
    let count = cell.restart_count();
    let supervisor = cell.parent();
    let decision = self.decide_catching(strategy, recipient, count, FailureKind::Killed);
    match decision {
      | Ok(Directive::Restart) => {
        if self.grant_restart(&mut cell) {
          self.restart_cell(recipient, cell, FailureKind::Killed, ACTOR_KILLED_CAUSE, ACTOR_KILLED_MESSAGE, None);
        } else {
          self.cells.push(cell);
          self.stop(recipient);
        }
      },
      | Ok(Directive::Stop) => {
        self.cells.push(cell);
        self.stop(recipient);
      },
      | Err(message) => {
        self.cells.push(cell);
        self.fail_supervisor(supervisor, recipient, &message);
      },
    }
    self.finish_boundary();
  }

  /// Builds an `Error` event with `cause_name` and `message`, raised by
  /// `target`.
  fn failure_reason(&self, target: &ActorRef, cause_name: &str, message: &str) -> Error {
    Error::with_cause_name(cause_name, target.path().to_string(), ACTOR_LOG_CLASS, message)
  }

  /// Publishes the failure of `target` as an error event.
  ///
  /// Mirrors `FaultHandling.logFailure`: the parent logs an `Error` for a child
  /// it restarts or stops, carrying the path of the child as its source. The
  /// event is built once and published to the system's event stream, which is
  /// the destination Pekko names; an installed sink is a test-side tap that
  /// receives the same single event.
  fn publish_child_failure(&self, target: &ActorRef, cause_name: &str, message: &str) {
    let event = self.failure_reason(target, cause_name, message);
    if let Some(stream) = self.event_stream.as_ref() {
      stream.publish(event.clone());
    }
    let sink = self.event_sink.lock().clone();
    if let Some(sink) = sink {
      sink.publish(&event);
    }
  }

  /// Publishes the report of a message an actor did not handle.
  ///
  /// Mirrors the publication `Actor.unhandled` makes: an [`UnhandledMessage`]
  /// carrying the message, the sender of the delivery, and the recipient, on the
  /// event stream. Pekko's `TestEventListener` turns that message into a
  /// `Warning` for the test logger; the port offers the same warning to an
  /// installed event sink, which is where a test reads it. A system without a
  /// stream or a sink drops the report without panicking.
  pub(crate) fn publish_unhandled(&self, message: Message, sender: ActorRef, recipient: ActorRef) {
    let unhandled = UnhandledMessage::new(message.clone(), sender.clone(), recipient.clone());
    if let Some(stream) = self.event_stream.as_ref() {
      stream.publish(unhandled);
    }
    let sink = self.event_sink.lock().clone();
    if let Some(sink) = sink {
      let source = recipient.path().to_string();
      let text = alloc::format!("unhandled message from {}: {:?}", sender.path(), message);
      sink.publish(&Warning::new(source, ACTOR_LOG_CLASS, &text));
    }
  }

  fn notify_watchers(&mut self, target: &ActorRef, existence_confirmed: bool, address_terminated: bool) {
    // Function references that watch through the dedicated table are
    // told apart by that table, not by the delivery branch (re-review
    // N4 of #192): they receive the notice as a message whose sender
    // is the ended target itself (design M5), and their registration
    // survives the notice until they unwatch or stop (design F3). A
    // watcher registered in both tables is told once, through this
    // table, so the notice keeps the original's sender (the ended
    // target) instead of the delivery table's (re-review 2, low 3
    // of #192).
    let function_watchers: Vec<ActorRef> = self
      .function_watching
      .iter()
      .filter(|(_, watched)| Self::same_watch_target(watched, target))
      .map(|(watcher, _)| watcher.clone())
      .collect();
    let watchers: Vec<ActorRef> = self
      .watchers
      .iter()
      .filter(|(_, watched)| Self::same_watch_target(watched, target))
      .map(|(watcher, _)| watcher.clone())
      .filter(|watcher| !function_watchers.contains(watcher))
      .collect();
    self.watchers.retain(|(_, watched)| !Self::same_watch_target(watched, target));
    for watcher in &watchers {
      self.enqueue_to(watcher.clone(), Terminated::new(target.clone(), existence_confirmed, address_terminated));
    }
    for watcher in function_watchers {
      let notice = Terminated::new(target.clone(), existence_confirmed, address_terminated);
      Envelope::new(watcher, Message::new(notice), target.clone()).post_to_boundary();
    }
  }

  /// The original and a detached reference can compare equal while addressing
  /// different delivery boundaries.
  fn same_watch_target(watched: &ActorRef, target: &ActorRef) -> bool {
    watched == target && watched.shares_boundary_with(target.boundary())
  }

  fn enqueue_to(&mut self, recipient: ActorRef, message: Terminated) {
    let sender = recipient.clone();
    if recipient.shares_boundary_with(self.queue.boundary()) {
      let pending = Arc::new(PendingTermination { watcher: recipient.clone(), notice: message });
      self.pending_terminations.push(pending.clone());
      self.queue.enqueue_locally(Envelope::new(recipient, Message::new(pending), sender));
    } else {
      Envelope::new(recipient, Message::new(message), sender).post_to_boundary();
    }
  }
}
