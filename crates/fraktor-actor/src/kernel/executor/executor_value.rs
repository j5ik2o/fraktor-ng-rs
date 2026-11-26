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
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::time::Duration;

use portable_atomic::AtomicBool;
use portable_atomic::AtomicU64;
use portable_atomic::Ordering;

use crate::kernel::actor::dispatch_settings::CapacityLimit;
use crate::kernel::actor::dispatch_settings::DispatchSettings;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::ActorCreationError;
use crate::kernel::actor::ActorSelection;
use crate::kernel::actor::ActorSelectionMessage;
use crate::kernel::actor::AnyDeadLetter;
use crate::kernel::actor::BehaviorStack;
use crate::kernel::actor::ConfigurationException;
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
use crate::kernel::actor_ref::next_actor_id;
use crate::kernel::actor_ref::next_actor_ref;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::dispatch::DispatcherKind;
use crate::kernel::dispatch::Dispatchers;
use crate::kernel::dispatch::Mailbox;
use crate::kernel::dispatch::MailboxCell;
use crate::kernel::dispatch::MailboxRun;
use crate::kernel::dispatch::ManualExecutor;
use crate::kernel::dispatch::ManualTask;
use crate::kernel::dispatch::Next;
use crate::kernel::dispatch::RunLimits;
use crate::kernel::dispatch::RunOutcome as MailboxOutcome;
use crate::kernel::dispatch::SystemMessage;
use crate::kernel::dispatch::Throughput;
use crate::kernel::dispatch::UnboundedQueue;
use crate::kernel::dispatch::DEFAULT_DISPATCHER_ID;
use crate::kernel::event::DeadLetterListener;
use crate::kernel::event::EventStream;
use crate::kernel::executor::ActorCell;
use crate::kernel::executor::ArrivalNotice;
use crate::kernel::executor::FunctionBoundary;
use crate::kernel::executor::LiveActors;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::supervision::Directive;
use crate::kernel::supervision::FailureCatcher;
use crate::kernel::supervision::FailureKind;
use crate::kernel::supervision::RestartHistory;
use crate::kernel::supervision::RestartLimit;
use crate::kernel::supervision::RunOutcome;
use crate::kernel::supervision::SupervisorStrategy;
use crate::kernel::sync::lock;
use crate::kernel::sync::Arc;
use crate::kernel::sync::SpinLockFactory;
use crate::kernel::sync::Weak;
use crate::kernel::Clock;

#[cfg(test)]
#[path = "executor_value_test.rs"]
mod tests;

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

/// The throughput of the dispatcher a test drives by hand, as Pekko's default
/// (`reference.conf:622`).
const MANUAL_THROUGHPUT: u32 = 5;

/// How long a wait on a boundary without a wake signal polls the boundary
/// before it looks at the mailboxes again.
const POLL_SLICE: Duration = Duration::from_millis(1);

/// A reply to send to `reply_to` after the actor it was registered on stops.
struct DeferredReply {
  reply_to: ActorRef,
  message: Message,
}

/// The key the executor finds an actor's entry by: the identifier and the
/// generation of its handle.
type ActorKey = (u64, u32);

/// What one actor of the system is, in one place (design of #234, section
/// 11.11.3).
///
/// The cell itself lives in the mailbox, which a run takes it out of. What
/// the executor reads and writes while the actor handles a message (the
/// stash, the replies it deferred, what it holds as a parent) stays here,
/// with the executor, until a context can borrow the cell it runs in
/// (stage 4).
struct ActorEntry {
  self_ref: ActorRef,
  mailbox: Arc<Mailbox<ActorCell>>,
  stash: Vec<(u64, Envelope)>,
  capacities: (CapacityLimit, CapacityLimit),
  supervision: Option<ParentEntry>,
  deferred_replies: Vec<DeferredReply>,
}

/// What a run left for the executor, to do once the run has returned
/// (design of #234, section 11.11.2).
enum AfterRun {
  /// `receive` failed under the catcher. `message` is the cause, `sender` the
  /// sender of the failed message, which `apply_failure` hands to the hooks as
  /// `sender()`.
  Failed { message: String, sender: ActorRef },
  /// `Kill`: the parent decides for a killed child (`handle_kill`).
  Kill,
  /// A stop by `stop_and_reply`. `sender` is the sender of the failed
  /// message when the message also failed.
  Stop { sender: Option<ActorRef> },
  /// `PoisonPill`: a stop at the boundary (`stop_at_boundary`), which holds
  /// the stops that the stop's own hooks ask for until it has finished.
  PoisonPill,
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
type TerminationCallback = Box<dyn Fn(&ActorRef) + Send>;

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
  custom: Option<Message>,
}

/// The head of a minimal router: it has no routees and routes nothing,
/// so every message it receives is reported as unhandled. Routing itself
/// is issue #269 (<https://github.com/j5ik2o/fraktor-ng-rs/issues/269>).
struct RouterHeadActor;

impl crate::kernel::actor::Actor for RouterHeadActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    context.unhandled(message);
  }
}

/// Validates one actor name the way every creation entry point must.
///
/// Mirrors `Children.checkName` together with
/// `ActorPath.validatePathElement`: the entry point itself refuses
/// only the empty name (Pekko also refuses null, which a Rust `&str`
/// cannot hold), and every other refusal is the one message of
/// `validate_path_element`.
pub(crate) fn validate_actor_name(name: &str) -> Result<(), InvalidActorNameException> {
  if name.is_empty() {
    return Err(InvalidActorNameException::empty_name());
  }
  ActorPath::validate_path_element(name).map_err(InvalidActorNameException::invalid_name)
}

/// Creates actors of one shard, runs their messages, and owns their state.
///
/// Mirrors the role of `org.apache.pekko.actor.ActorCell` together with a
/// dispatcher a caller drives by hand: every actor has its own mailbox, a
/// mailbox that has work waits in the [`ManualExecutor`], and a
/// `ShardBoundary` carries what has no mailbox (design of #234, section
/// 11.11). The executor alone owns the actors' entries; a caller only holds
/// `ActorRef` handles.
///
/// The executor also owns whether the system has been terminated. Keeping that
/// one fact here lets every entry that starts a new actor, including the ones
/// that reach the executor through a weak handle, refuse the request with the
/// same cause as Pekko.
pub(crate) struct Executor {
  actors: BTreeMap<ActorKey, ActorEntry>,
  ref_parents: Vec<ParentEntry>,
  boundary: Arc<dyn ShardBoundary>,
  manual: Arc<ManualExecutor<ActorCell>>,
  live: Arc<LiveActors>,
  arrivals: Option<Arc<ArrivalNotice>>,
  after_run: Option<(ActorKey, AfterRun)>,
  in_run: bool,
  fatal: bool,
  root: ActorPath,
  watchers: Vec<(ActorRef, ActorRef)>,
  pending_terminations: Vec<Arc<PendingTermination>>,
  function_refs: Vec<ActorRef>,
  catcher: Option<Arc<dyn FailureCatcher>>,
  event_sink: Arc<spin::Mutex<Option<Arc<dyn EventSink>>>>,
  event_stream: Option<EventStream>,
  pending_stops: Vec<ActorRef>,
  pending_decider_failures: Vec<(ActorRef, String)>,
  pending_child_terminated: Vec<(ActorRef, ActorRef)>,
  running: Vec<ActorKey>,
  termination_callbacks: Vec<TerminationCallback>,
  dead_letters: ActorRef,
  dead_letters_shared: Arc<DeadLettersShared>,
  dead_letter_listener: Option<ActorRef>,
  boundary_active: bool,
  hook_child_creation_refused: bool,
  terminated: bool,
  routes_closed: Arc<AtomicBool>,
  clock: Option<Arc<dyn Clock>>,
  next_delivery: u64,
  dispatch_settings: Option<DispatchSettings>,
  function_parents: Vec<(ActorRef, ActorRef)>,
  function_watching: Vec<(ActorRef, ActorRef)>,
  root_guardian: ActorRef,
  user_guardian: ActorRef,
  system_guardian: ActorRef,
  dispatchers: Dispatchers,
  watch_messages: Vec<(ActorRef, ActorRef, Message)>,
  user_children: BTreeMap<String, ActorRef>,
}

impl Executor {
  pub(crate) fn new(root: ActorPath, boundary: Arc<dyn ShardBoundary>) -> Self {
    let dead_letters_shared = Arc::new(DeadLettersShared::new());
    let dead_letters = Self::create_dead_letters(&root, dead_letters_shared.clone());
    // The guardians are handles without cells: Pekko's guardians are
    // real actors, but this port keeps no actor behind them, so a
    // message addressed to a guardian path is reported as unhandled
    // (see `deliver` and `deliver_selection`). The top-level names
    // register in `user_children`, apart from the parent registry,
    // whose entries belong to actors that can stop.
    let root_guardian = next_actor_ref(root.clone(), boundary.clone());
    let user_guardian = next_actor_ref(root.with_child("user"), boundary.clone());
    let system_guardian = next_actor_ref(root.with_child("system"), boundary.clone());
    let routes_closed = Arc::new(AtomicBool::new(false));
    let signal = boundary.wake_signal();
    let key = Arc::as_ptr(&boundary) as *const () as usize;
    let manual = Arc::new(ManualExecutor::new(key, signal.clone(), &SpinLockFactory));
    let arrivals = signal.map(|signal| Arc::new(ArrivalNotice::new(signal)));
    let mut executor = Self {
      actors: BTreeMap::new(),
      ref_parents: Vec::new(),
      boundary,
      manual,
      live: Arc::new(LiveActors::new(&SpinLockFactory)),
      arrivals,
      after_run: None,
      in_run: false,
      fatal: false,
      root,
      watchers: Vec::new(),
      pending_terminations: Vec::new(),
      function_refs: Vec::new(),
      catcher: None,
      event_sink: Arc::new(spin::Mutex::new(None)),
      event_stream: None,
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
      routes_closed,
      clock: None,
      next_delivery: 0,
      dispatch_settings: None,
      function_parents: Vec::new(),
      function_watching: Vec::new(),
      root_guardian,
      user_guardian,
      system_guardian,
      dispatchers: Dispatchers::with_defaults(),
      watch_messages: Vec::new(),
      user_children: BTreeMap::new(),
    };
    executor.function_refs.push(executor.dead_letters.clone());
    executor
  }

  /// Returns the entry of `target`, while the actor has not stopped.
  fn entry(&self, target: &ActorRef) -> Option<&ActorEntry> {
    self.actors.get(&target.key())
  }

  fn entry_mut(&mut self, target: &ActorRef) -> Option<&mut ActorEntry> {
    self.actors.get_mut(&target.key())
  }

  /// Returns whether the cell of `target` is in its mailbox: the actor has
  /// not stopped, and neither a run nor the executor has the cell out.
  fn has_cell(&self, target: &ActorRef) -> bool {
    self.entry(target).is_some_and(|entry| entry.mailbox.has_cell())
  }

  /// Takes the cell of `target` out of its mailbox, when it is there.
  fn take_cell(&self, target: &ActorRef) -> Option<Box<ActorCell>> {
    self.entry(target)?.mailbox.try_take_cell()
  }

  /// Puts a cell the executor took out back into its mailbox.
  fn put_cell(&self, cell: Box<ActorCell>) {
    let entry = self.actors.get(&cell.key()).expect("an actor keeps its entry until it stops");
    entry.mailbox.put_back_cell(cell);
  }

  /// Reads the cell of `target` while it is in its mailbox.
  fn with_cell<R>(&self, target: &ActorRef, read: impl FnOnce(&ActorCell) -> R) -> Option<R> {
    self.entry(target)?.mailbox.with_cell(read)
  }

  /// Changes the cell of `target` while it is in its mailbox.
  fn with_cell_mut<R>(&self, target: &ActorRef, change: impl FnOnce(&mut ActorCell) -> R) -> Option<R> {
    self.entry(target)?.mailbox.with_cell_mut(change)
  }

  /// Returns the handles of the actors whose cell is in its mailbox and
  /// passes `filter`, in the order the actors were created.
  fn cells_where(&self, filter: impl Fn(&ActorCell) -> bool) -> Vec<ActorRef> {
    self
      .actors
      .values()
      .filter(|entry| entry.mailbox.with_cell(&filter) == Some(true))
      .map(|entry| entry.self_ref.clone())
      .collect()
  }

  /// Returns the children of `parent` whose cell is in its mailbox.
  fn children_of(&self, parent: &ActorRef) -> Vec<ActorRef> {
    self.cells_where(|cell| cell.is_child_of(parent))
  }

  /// Returns what the executor keeps for `parent` as a parent. A function
  /// reference has no entry, so its record is kept apart.
  fn parent_entry(&self, parent: &ActorRef) -> Option<&ParentEntry> {
    match self.actors.get(&parent.key()) {
      | Some(entry) => entry.supervision.as_ref(),
      | None => self.ref_parents.iter().find(|entry| &entry.parent == parent),
    }
  }

  fn parent_entry_mut(&mut self, parent: &ActorRef) -> Option<&mut ParentEntry> {
    match self.actors.get_mut(&parent.key()) {
      | Some(entry) => entry.supervision.as_mut(),
      | None => self.ref_parents.iter_mut().find(|entry| &entry.parent == parent),
    }
  }

  fn insert_parent_entry(&mut self, record: ParentEntry) {
    match self.actors.get_mut(&record.parent.key()) {
      | Some(entry) => entry.supervision = Some(record),
      | None => self.ref_parents.push(record),
    }
  }

  /// Returns whether `target` belongs to this system: its mailbox is run by
  /// this executor, or it posts to this system's boundary.
  fn is_local(&self, target: &ActorRef) -> bool {
    target.system_key() == self.manual.key()
  }

  /// Creates the boundary of a function reference, which tells a waiting
  /// caller when its handler has run.
  fn function_boundary(&self, boundary: FunctionBoundary) -> Arc<dyn ShardBoundary> {
    match &self.arrivals {
      | Some(arrivals) => Arc::new(boundary.with_arrivals(Arc::clone(arrivals))),
      | None => Arc::new(boundary),
    }
  }

  /// Registers `callback` to run with the handle of every actor that stops.
  ///
  /// Mirrors the notification that Pekko's `ActorClassificationUnsubscriber`
  /// receives for a watched subscriber: the executor is the only place that
  /// knows an actor has stopped, so a component that is not an actor registers
  /// here. The callback runs after the watchers have been told and must not
  /// lock the executor, which is already locked by the caller.
  pub(crate) fn on_termination<F>(&mut self, callback: F)
  where
    F: Fn(&ActorRef) + Send + 'static,
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
    let id = next_actor_id();
    let path = Arc::new(path);
    let reference = ActorRef::new(id, 0, path.clone(), boundary.clone());
    slot.call_once(|| (id, path, Arc::downgrade(&boundary)));
    reference
  }

  /// Builds the route a refused envelope takes (issue #286): the dead
  /// letter rules, run on the sender's thread, as Pekko's dead letter
  /// mailbox sends `DeadLetter(message, sender, receiver)` to `deadLetters`
  /// from its `enqueue`. The route captures the routing state and the dead
  /// letters handle once, when the actor is spawned, and a routing passes
  /// the handle by reference, so routing one envelope clones no handle of
  /// the executor's and walks no way back (design of #234, section
  /// 11.11.7, the refused path of condition 1).
  fn dead_letter_route(&self) -> Arc<dyn Fn(Envelope) + Send + Sync> {
    let shared = self.dead_letters_shared.clone();
    let dead_letters = self.dead_letters.clone();
    let routes_closed = self.routes_closed.clone();
    Arc::new(move |envelope: Envelope| {
      // Once the system has terminated, a refused envelope is discarded,
      // as the closed executor queue discarded it before the routing moved
      // to the sender's thread (review of PR #295).
      if routes_closed.load(Ordering::SeqCst) {
        return;
      }
      let (recipient, message, sender) = envelope.into_parts();
      if let Some(recipient) = recipient {
        shared.route_undeliverable(message, sender, recipient, &dead_letters);
      }
    })
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
    ActorSelection::new(self.boundary.clone(), self.dead_letters.clone(), path, Arc::downgrade(&self.live))
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
    let boundary = self.boundary.clone();
    let now_of = Arc::new(move || -> Duration {
      match clock.as_ref() {
        | Some(clock) => clock.monotonic(),
        | None => boundary.monotonic_time().unwrap_or(Duration::ZERO),
      }
    });
    let bus = self.event_stream.as_ref().map(|stream| stream.downgrade());
    let sink = self.event_sink.clone();
    let state = Arc::new(spin::Mutex::new(DeadLetterListener::new(settings, dead_letters.clone())));
    let post_boundary = self.boundary.clone();
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
    let boundary = self.function_boundary(FunctionBoundary::new(Arc::new(handler)));
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
      .or_else(|| self.boundary.monotonic_time())
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

  /// Installs the dispatcher registry that creation verifies against.
  pub(crate) fn set_dispatchers(&mut self, dispatchers: Dispatchers) {
    self.dispatchers = dispatchers;
  }

  /// Returns whether `id` names a configured dispatcher: either the
  /// registry installed for this system, or a definition (or alias) in
  /// the system's `DispatchSettings`, whose definitions the ported
  /// tests configure their dispatchers through. A name whose alias
  /// chain cannot be resolved is not configured, even when the
  /// registry table holds the name: creation resolves the same chain
  /// to find the actor's capacities, so passing verification here
  /// would only move the failure into creation.
  fn has_dispatcher(&self, id: &str) -> bool {
    if let Some(settings) = &self.dispatch_settings {
      match settings.resolve_dispatcher(id) {
        | Ok(Some(_)) => return true,
        | Ok(None) => {},
        | Err(_) => return false,
      }
    }
    self.dispatchers.has_dispatcher(id)
  }

  /// Verifies the configuration of a creation at `path`, in Pekko's
  /// order: without a router, the recipe's dispatcher must be
  /// configured; with a router, the routees' dispatcher is checked
  /// first, then the head's, and a balancing head is refused. The head
  /// message names the head dispatcher; Pekko's body puts the routee
  /// name there by mistake, and the port does not copy the mistake.
  fn verify_configuration(&self, props: &Props, path: &ActorPath) -> Result<(), ConfigurationException> {
    let routee_dispatcher = props.dispatcher().unwrap_or(DEFAULT_DISPATCHER_ID);
    match props.router() {
      | None => {
        if !self.has_dispatcher(routee_dispatcher) {
          return Err(ConfigurationException::new(alloc::format!(
            "Dispatcher [{routee_dispatcher}] not configured for path {path}"
          )));
        }
        Ok(())
      },
      | Some(router) => {
        if !self.has_dispatcher(routee_dispatcher) {
          return Err(ConfigurationException::new(alloc::format!(
            "Dispatcher [{routee_dispatcher}] not configured for routees of {path}"
          )));
        }
        let head_dispatcher = router.router_dispatcher.as_deref().unwrap_or(DEFAULT_DISPATCHER_ID);
        if !self.has_dispatcher(head_dispatcher) {
          return Err(ConfigurationException::new(alloc::format!(
            "Dispatcher [{head_dispatcher}] not configured for router of {path}"
          )));
        }
        if self.dispatchers.kind_of(head_dispatcher) == Some(DispatcherKind::Balancing) {
          return Err(ConfigurationException::new(alloc::format!(
            "Configuration for {path} is invalid - you can not use a 'BalancingDispatcher' as a Router's dispatcher, you can however use it for the routees."
          )));
        }
        Ok(())
      },
    }
  }

  /// Resolves `path` to a handle, as `ActorRefProvider.resolveActorRef`
  /// does: a live actor's own handle, a guardian's handle for a
  /// guardian path, and otherwise a handle that keeps the path and
  /// answers nothing (Pekko's `EmptyLocalActorRef`). Dead letters is
  /// returned only for a path whose root belongs to another system.
  /// The actor currently running (out of the cell table) resolves too.
  pub(crate) fn resolve_actor_ref(&self, path: &ActorPath) -> ActorRef {
    if path.root() != self.root.root() {
      return self.dead_letters.clone();
    }
    // Whole-path comparison, for the same reason as `guardian_for_path`:
    // `elements` answers one empty element for the root path.
    if let Some(guardian) = self.guardian_for_path(path) {
      return guardian;
    }
    if let Some(reference) = self.live_actor_at(path) {
      return reference;
    }
    if let Some(reference) = self.function_refs.iter().find(|reference| reference.has_path(path)) {
      return reference.clone();
    }
    next_actor_ref(path.clone(), self.boundary.clone())
  }

  /// Returns the guardian handle for a guardian `path`, when it is one.
  fn guardian_for_path(&self, path: &ActorPath) -> Option<ActorRef> {
    // Whole-path comparison: `ActorPath::elements` answers one empty
    // element for the root path (Pekko's own convention), so it cannot
    // tell the root from a one-element path here.
    if *path == self.root {
      Some(self.root_guardian.clone())
    } else if *path == self.root.with_child("user") {
      Some(self.user_guardian.clone())
    } else if *path == self.root.with_child("system") {
      Some(self.system_guardian.clone())
    } else {
      None
    }
  }

  /// Installs the dispatcher and mailbox definitions that spawn-time
  /// resolution reads, from the system's [`DispatchSettings`] setup.
  pub(crate) fn set_dispatch_settings(&mut self, settings: DispatchSettings) {
    self.dispatch_settings = Some(settings);
  }

  /// Returns the (mailbox, stash) capacities resolved for `owner` when
  /// it was spawned, both unlimited when no definition applied.
  fn capacities_of(&self, owner: &ActorRef) -> (CapacityLimit, CapacityLimit) {
    self.entry(owner).map_or((CapacityLimit::Unlimited, CapacityLimit::Unlimited), |entry| entry.capacities)
  }

  /// Publishes a [`DeadLetter`] for `message` on the event stream and
  /// hands the message to the dead letters handle, as Pekko's mailbox
  /// clean-up both publishes and delivers.
  fn publish_dead_letter(&mut self, message: Message, sender: ActorRef, recipient: &ActorRef) {
    // The dead letters handle publishes the wrapped event as it is, as
    // Pekko's mailbox clean-up paths do.
    let wrapped = DeadLetter::from_message(message, sender.clone(), recipient.clone());
    let dead_letters = self.dead_letters();
    self.send(sender, &dead_letters, Message::new(wrapped));
  }

  /// Starts a top-level actor after both the name checks and the
  /// configuration checks, as `ActorSystem.actorOf` does in Pekko,
  /// which throws both refusal kinds from the one call.
  ///
  /// The name is checked first (`Children.checkName`), then system
  /// liveness and name uniqueness, then dispatcher and router
  /// configuration. With a router, the head that starts is the minimal
  /// router head: verification has passed, no routees are created, and
  /// a message to the head is reported as unhandled. A named top-level
  /// actor is recorded in `user_children` only if it stays live past
  /// `pre_start`, and `forget_child` removes it when it stops, so its
  /// name is immediately free for reuse.
  pub(crate) fn try_spawn_user(&mut self, props: Props, name: Option<&str>) -> Result<ActorRef, ActorCreationError> {
    if let Some(name) = name {
      validate_actor_name(name)?;
    }
    if self.terminated {
      return Err(InvalidActorNameException::terminating().into());
    }
    let label = match name {
      | Some(name) => name.to_string(),
      | None => alloc::format!("${}", next_spawn_name()),
    };
    let path = self.root.with_child("user").with_child(&label);
    if name.is_some() && self.user_children.contains_key(&label) {
      return Err(InvalidActorNameException::name_taken(&label).into());
    }
    self.verify_configuration(&props, &path)?;
    let head_props = match props.router() {
      | Some(router) => {
        let head = router.router_dispatcher.as_deref().unwrap_or(DEFAULT_DISPATCHER_ID);
        Props::new(|| RouterHeadActor).with_dispatcher(head)
      },
      | None => props,
    };
    let reference = self.spawn_at(head_props, path, None, SupervisorStrategy::default_strategy(), false);
    if name.is_some() && self.has_cell(&reference) {
      self.user_children.insert(label, reference.clone());
    }
    Ok(reference)
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
      !self.parent_entry(parent).is_some_and(|entry| entry.stopping),
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
    // keeps the unlimited behaviour.
    let capacities = match &self.dispatch_settings {
      | Some(settings) => match settings.resolve_capacities(props.dispatcher(), props.mailbox()) {
        | Ok(capacities) => capacities,
        | Err(error) => panic!("{error}"),
      },
      | None => (CapacityLimit::Unlimited, CapacityLimit::Unlimited),
    };
    let queue = Arc::new(UnboundedQueue::new(&SpinLockFactory));
    let limits = RunLimits::new(Throughput::new(MANUAL_THROUGHPUT));
    let mut mailbox = Mailbox::for_actor(queue, limits, &SpinLockFactory, Arc::downgrade(&self.manual));
    mailbox.set_refused_route(self.dead_letter_route());
    let mailbox = Arc::new(mailbox);
    let self_ref = ActorRef::for_mailbox(next_actor_id(), Arc::new(path), Arc::clone(&mailbox));
    let cell = ActorCell::pending(self_ref.clone(), parent, props, strategy, strategy_inherited);
    mailbox.put_back_cell(Box::new(cell));
    let entry = ActorEntry {
      self_ref: self_ref.clone(),
      mailbox,
      stash: Vec::new(),
      capacities,
      supervision: None,
      deferred_replies: Vec::new(),
    };
    self.actors.insert(self_ref.key(), entry);
    self.live.insert(self_ref.clone());
    self.create_initial(&self_ref);
    self_ref
  }

  /// Records the strategy `parent` applies to the children it starts.
  ///
  /// Mirrors the `supervisorStrategy` a Pekko parent keeps for its `Children`.
  /// The executor stores it apart from the parent's cell because the cell is
  /// out of the table while the parent handles a message.
  pub(crate) fn register_parent_strategy(&mut self, parent: ActorRef, strategy: SupervisorStrategy) {
    self.require_restart_clock(&strategy);
    match self.parent_entry_mut(&parent) {
      | Some(entry) => entry.strategy = strategy,
      | None => self.insert_parent_entry(ParentEntry { parent, strategy, children: Vec::new(), stopping: false }),
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
    let Some(strategy) = self.parent_entry(parent).map(|entry| entry.strategy.clone()) else {
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
    if self.parent_entry(parent).is_none() {
      if !self.is_live(parent) && !self.is_guardian(parent) {
        return Err(InvalidActorNameException::terminating());
      }
      self.insert_parent_entry(ParentEntry {
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
    let Some(entry) = self.parent_entry(parent) else {
      return Err(InvalidActorNameException::terminating());
    };
    if self.terminated || entry.stopping {
      self.hook_child_creation_refused = true;
      return Err(InvalidActorNameException::terminating());
    }
    validate_actor_name(name)?;
    if entry.children.iter().any(|(used, _)| used == name) {
      return Err(InvalidActorNameException::name_taken(name));
    }
    let label = name.to_string();
    let child = self.spawn_child_with_origin(parent, props, Some(&label), strategy, strategy_inherited);
    if self.has_cell(&child) {
      if let Some(entry) = self.parent_entry_mut(parent) {
        entry.children.push((label, child.clone()));
      }
    }
    Ok(child)
  }

  /// Starts a child for a caller that only holds a non-owning handle and
  /// wants a refusal as a value. The checks are the ones
  /// [`try_spawn_user`](Self::try_spawn_user) runs for a top-level
  /// creation, in the same order: the name first, then that the system
  /// and the parent still take children, then that the name is free,
  /// then the configuration. A child creation therefore refuses for the
  /// same reasons whichever entry a caller reaches it through, instead
  /// of one entry checking the configuration and another skipping it.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name, the parent's state,
  /// or the configuration is refused.
  pub(crate) fn try_spawn_child_from_weak(
    &mut self,
    parent: &ActorRef,
    props: Props,
    name: Option<&str>,
    strategy: SupervisorStrategy,
  ) -> Result<ActorRef, ActorCreationError> {
    if let Some(name) = name {
      validate_actor_name(name)?;
    }
    if self.terminated {
      return Err(InvalidActorNameException::terminating().into());
    }
    match self.parent_entry(parent) {
      | Some(entry) => {
        if entry.stopping {
          return Err(InvalidActorNameException::terminating().into());
        }
        if let Some(name) = name {
          if entry.children.iter().any(|(used, _)| used == name) {
            return Err(InvalidActorNameException::name_taken(name).into());
          }
        }
      },
      | None => {
        if !self.is_live(parent) && !self.is_guardian(parent) {
          return Err(InvalidActorNameException::terminating().into());
        }
      },
    }
    // A child of the user guardian is a top-level actor, so its name must
    // also be free in the top-level registry, whether or not the guardian
    // already keeps a record of its own children.
    if parent == &self.user_guardian {
      if let Some(name) = name {
        if self.user_children.contains_key(name) {
          return Err(InvalidActorNameException::name_taken(name).into());
        }
      }
    }
    // The configuration check reads only the recipe and the path. For
    // an unnamed child the last path segment is the generated name,
    // which the creation itself draws, so the check runs against a
    // placeholder segment that only an error message would show.
    let path = match name {
      | Some(name) => parent.path().with_child(name),
      | None => parent.path().with_child("$generated"),
    };
    self.verify_configuration(&props, &path)?;
    let child = match name {
      | Some(name) => self.spawn_named_child_with_strategy(parent, props, name, strategy)?,
      | None => self.spawn_child(parent, props, None, strategy),
    };
    // A named child of the user guardian is recorded with the
    // top-level actors, as `try_spawn_user` records its own: the name
    // stays taken for both entries until the child stops, and
    // `forget_child` frees it for both.
    if let Some(name) = name {
      if parent == &self.user_guardian && self.has_cell(&child) {
        self.user_children.insert(name.to_string(), child.clone());
      }
    }
    Ok(child)
  }

  /// Records a stop request to carry out after the actor that asked returns.
  pub(crate) fn request_stop(&mut self, target: ActorRef) {
    self.pending_stops.push(target);
  }

  /// Marks the start of one actor execution frame.
  fn enter_frame(&mut self, target: &ActorRef) {
    self.running.push(target.key());
  }

  /// Marks the end of one actor execution frame.
  ///
  /// The frame is only popped here; the stops and child-terminated notices it
  /// left behind run once the outermost frame has returned, because the actor
  /// that asked for them is out of the cell table until then.
  fn exit_frame(&mut self) {
    self.running.pop();
  }

  /// Returns whether `target` runs a frame: its cell is out of its mailbox
  /// while it handles a message or runs a hook.
  fn is_running(&self, target: &ActorRef) -> bool {
    let key = target.key();
    self.running.contains(&key)
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
      let is_child = self.with_cell(&target, |cell| cell.is_child_of(parent)) == Some(true);
      let is_running = self.is_running(&target);
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
      let Some(index) = self.pending_child_terminated.iter().position(|(parent, _)| !self.is_running(parent)) else {
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
  fn create_initial(&mut self, self_ref: &ActorRef) {
    loop {
      let catcher = self.catcher.clone();
      let mut cell = self.take_cell(self_ref).expect("a cell being created is in its mailbox");
      let mut stack = cell.take_stack();
      self.enter_frame(self_ref);
      let outcome = {
        let mut context = ActorContext::new(self_ref, self_ref.clone(), self, &mut stack);
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
          self.put_cell(cell);
          self.finish_boundary();
          return;
        },
        | RunOutcome::Failed { message } => {
          // Pekko's `failActor` clears the behaviour stack before a retry, so a
          // `become` issued by a `pre_start` that threw does not survive into
          // the next creation. Dropping `stack` leaves the fresh empty one that
          // `take_stack` put in its place.
          self.put_cell(cell);
          self.publish_child_failure(self_ref, ACTOR_INITIALIZATION_CAUSE, &message);
          let strategy = self.strategy_of(self_ref);
          if strategy.is_all_for_one() {
            let retry = self.apply_all_for_one(
              self_ref,
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
            self.stop_children_of(self_ref);
            if !self.has_cell(self_ref) {
              self.finish_boundary();
              return;
            }
            continue;
          }
          let count = self.with_cell(self_ref, ActorCell::restart_count).expect("the failed cell is back");
          let decision = self.decide_catching(strategy, self_ref, count, FailureKind::Initialization);
          match decision {
            | Ok(Directive::Restart) => {
              if !self.grant_restart_of(self_ref) {
                self.stop(self_ref);
                self.finish_boundary();
                return;
              }
              self.stop_children_of(self_ref);
              if !self.has_cell(self_ref) {
                self.finish_boundary();
                return;
              }
            },
            | Ok(Directive::Stop) => {
              self.stop(self_ref);
              self.finish_boundary();
              return;
            },
            | Err(message) => {
              let supervisor = self.with_cell(self_ref, ActorCell::parent).flatten();
              self.fail_supervisor(supervisor, self_ref, &message);
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
    for child in self.children_of(parent) {
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
        if let Some(entry) = self.parent_entry(&parent) {
          return entry.strategy.clone();
        }
      }
    }
    cell.strategy().clone()
  }

  /// Returns the strategy that decides for `target`, whose cell is in its
  /// mailbox, as [`effective_strategy`](Self::effective_strategy) does for a
  /// taken cell.
  fn strategy_of(&self, target: &ActorRef) -> SupervisorStrategy {
    let (inherited, parent, own) = self
      .with_cell(target, |cell| (cell.strategy_inherited(), cell.parent(), cell.strategy().clone()))
      .expect("the cell that a strategy decides for is in its mailbox");
    if inherited {
      if let Some(entry) = parent.as_ref().and_then(|parent| self.parent_entry(parent)) {
        return entry.strategy.clone();
      }
    }
    own
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
    let is_cell = self.has_cell(target);
    let is_function_ref = self.function_refs.contains(target);
    if !is_cell && !is_function_ref {
      return;
    }
    self.mark_stopping(target);
    let parent_of_target = self.with_cell(target, ActorCell::parent).flatten();
    self.stop_children_of(target);
    self.stop_function_refs_of(target);
    self.close_function_ref(target);
    if let Some(mut cell) = self.take_cell(target) {
      if !cell.post_stop_suppressed() {
        self.run_post_stop(&mut cell, sender);
      }
    }
    // A post_stop hook receives a context and can create function
    // references; those belong to the stopping actor and end here too.
    // References already ended above are gone from the registry, so
    // this second pass never notifies a watcher twice.
    self.stop_function_refs_of(target);
    self.forget_child(target, parent_of_target.as_ref());
    self.stash_to_dead_letters(target);
    self.queued_to_dead_letters(target);
    self.finish_termination(target);
    self.flush_deferred_replies(target);
    if let Some(parent) = parent_of_target {
      self.run_child_terminated_hook(&parent, target);
    }
    let entry = self.actors.remove(&target.key());
    self.ref_parents.retain(|entry| &entry.parent != target);
    drop(entry);
  }

  /// Marks `target` as stopping in the parent registry.
  fn mark_stopping(&mut self, target: &ActorRef) {
    if let Some(entry) = self.parent_entry_mut(target) {
      entry.stopping = true;
    }
  }

  /// Forgets the names the stopped actor held as a child of its parent.
  fn forget_child(&mut self, child: &ActorRef, parent: Option<&ActorRef>) {
    if let Some(entry) = parent.and_then(|parent| self.parent_entry_mut(parent)) {
      entry.children.retain(|(_, held)| held != child);
    }
    let child_path = child.path();
    if self.user_children.get(child_path.name()) == Some(child) {
      self.user_children.remove(child_path.name());
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
    let Some(entry) = self.parent_entry(parent) else {
      return;
    };
    if !entry.strategy.has_child_terminated() {
      return;
    }
    if self.is_running(parent) {
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
    let Some(entry) = self.parent_entry(parent) else {
      return;
    };
    if !entry.strategy.has_child_terminated() {
      return;
    }
    let strategy = entry.strategy.clone();
    let siblings = self.children_of(parent);
    if self.with_cell(parent, ActorCell::is_created) != Some(true) {
      return;
    }
    let mut cell = self.take_cell(parent).expect("the parent's cell was just read in its mailbox");
    let mut stack = cell.take_stack();
    self.hook_child_creation_refused = false;
    self.enter_frame(parent);
    let catcher = self.catcher.clone();
    let outcome = {
      let mut context = ActorContext::new(parent, parent.clone(), self, &mut stack);
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
    self.put_cell(cell);
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
          let mut context = ActorContext::new(&self_ref, hook_sender, self, &mut stack);
          catcher.run(&mut || cell.post_stop_with_context(&mut context))
        };
        if let RunOutcome::Failed { message } = ran {
          self.publish_child_failure(&self_ref, EXCEPTION_CAUSE, &message);
        }
      },
      | None => {
        let mut context = ActorContext::new(&self_ref, hook_sender, self, &mut stack);
        cell.post_stop_with_context(&mut context);
      },
    }
    self.exit_frame();
  }

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
  /// as it is a contract violation rather than a capacity one.
  pub(crate) fn try_stash(
    &mut self,
    owner: ActorRef,
    delivery: u64,
    message: Message,
    sender: ActorRef,
  ) -> Result<(), StashOverflowException> {
    let (_, stash_capacity) = self.capacities_of(&owner);
    let entry = self.entry_mut(&owner).expect("only an actor with an entry stashes");
    assert!(
      entry.stash.last().is_none_or(|(last, _)| *last != delivery),
      "Can't stash the same message more than once"
    );
    if !stash_capacity.allows(entry.stash.len()) {
      return Err(StashOverflowException::new(stash_capacity.bound().unwrap_or(0)));
    }
    entry.stash.push((delivery, Envelope::without_recipient(message, sender)));
    Ok(())
  }

  /// Publishes every envelope in `envelopes` as a dead letter for
  /// `recipient`, preserving each envelope's original sender.
  fn publish_envelopes_to_dead_letters(&mut self, envelopes: impl IntoIterator<Item = Envelope>, recipient: &ActorRef) {
    for envelope in envelopes {
      let (message, sender) = envelope.into_message();
      self.publish_dead_letter(message, sender, recipient);
    }
  }

  /// Applies `owner`'s bounded mailbox capacity to a returning stash:
  /// the oldest excess envelopes go to dead letters (newest victim
  /// first), and the newest envelopes that fit are returned in stash
  /// order.
  fn discard_stash_overflow(&mut self, owner: &ActorRef, stash: Vec<(u64, Envelope)>) -> Vec<Envelope> {
    let (mailbox_capacity, _) = self.capacities_of(owner);
    let room = mailbox_capacity
      .bound()
      .map(|bound| bound.saturating_sub(self.entry(owner).map_or(0, |entry| entry.mailbox.number_of_messages())))
      .unwrap_or(usize::MAX);
    let total = stash.len();
    let mut envelopes = stash.into_iter().map(|(_, envelope)| envelope);
    if total > room {
      let victims: Vec<_> = envelopes.by_ref().take(total - room).collect();
      self.publish_envelopes_to_dead_letters(victims.into_iter().rev(), owner);
    }
    envelopes.collect()
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
    let stash = self.entry_mut(owner).map(|entry| core::mem::take(&mut entry.stash)).unwrap_or_default();
    if !stash.is_empty() {
      // Pekko's postStop returns the stash to the bounded mailbox: the
      // oldest excess goes to dead letters first (newest victim first),
      // and the newest messages that fit stay in the mailbox and are
      // cleaned up in order. An unlimited mailbox sends everything in
      // order.
      let remaining = self.discard_stash_overflow(owner, stash);
      self.publish_envelopes_to_dead_letters(remaining, owner);
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
    let Some(entry) = self.entry(target) else {
      return;
    };
    self.live.remove(target);
    // The system messages left (a Resume at most, in stage 3) mean nothing
    // to a closed mailbox, and the runs that wait for it would find it
    // closed: both are dropped here, outside every lock.
    let (envelopes, system) = entry.mailbox.close();
    let runs = self.manual.take_runs_of(&entry.mailbox);
    drop(system);
    drop(runs);
    for envelope in envelopes {
      let (message, sender) = envelope.into_message();
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
    let stash = self.entry_mut(owner).map(|entry| core::mem::take(&mut entry.stash)).unwrap_or_default();
    if stash.is_empty() {
      return;
    }
    // The bounded mailbox bounds this return, as Pekko's
    // `enqueueFirst` into a bounded deque does: the messages already
    // queued for the owner keep their room, the newest stashed
    // messages that fit are prepended, and the oldest excess goes to
    // dead letters, newest victim first. An unlimited mailbox returns
    // everything.
    let remaining = self.discard_stash_overflow(owner, stash);
    let envelopes: Vec<Envelope> = remaining.into_iter().map(|envelope| self.requeue_notice(owner, envelope)).collect();
    let refused = match self.entry(owner) {
      | Some(entry) => Mailbox::send_first(&entry.mailbox, envelopes).err().unwrap_or_default(),
      | None => envelopes,
    };
    self.publish_envelopes_to_dead_letters(refused, owner);
  }

  fn requeue_notice(&mut self, owner: &ActorRef, envelope: Envelope) -> Envelope {
    let (message, sender) = envelope.into_message();
    let message = match message.downcast_ref::<Terminated>() {
      | Some(notice) => {
        let queued = self.pending_terminations.iter().find(|pending| {
          &pending.watcher == owner && Self::same_watch_target(pending.notice.target(), notice.target())
        });
        let pending = match queued {
          | Some(pending) => pending.clone(),
          | None => {
            let pending = Arc::new(PendingTermination { watcher: owner.clone(), notice: notice.clone(), custom: None });
            self.pending_terminations.push(pending.clone());
            pending
          },
        };
        Message::new(pending)
      },
      | None => message,
    };
    Envelope::without_recipient(message, sender)
  }

  /// Registers a reply to `reply_to` after `target` stops.
  pub(crate) fn stop_and_reply(&mut self, target: &ActorRef, reply_to: ActorRef, message: Message) {
    let entry = self.entry_mut(target).expect("only an actor with an entry stops after a reply");
    entry.deferred_replies.push(DeferredReply { reply_to, message });
  }

  /// Sends the replies that `target` deferred when it stopped.
  fn flush_deferred_replies(&mut self, target: &ActorRef) {
    let ready = self.entry_mut(target).map(|entry| core::mem::take(&mut entry.deferred_replies)).unwrap_or_default();
    for DeferredReply { reply_to, message } in ready {
      let sender = reply_to.clone();
      reply_to.send_message(message, sender);
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
    let dead_letters = self.dead_letters();
    let boundary = self.function_boundary(FunctionBoundary::new_with_dead_letters(Arc::new(handler), dead_letters));
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
    // another cell.
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

  /// Removes `target` from the live function reference table and closes
  /// its boundary.
  fn close_function_ref(&mut self, target: &ActorRef) {
    if let Some(index) = self.function_refs.iter().position(|reference| reference == target) {
      let reference = self.function_refs.remove(index);
      if let Some(boundary) = reference.boundary() {
        boundary.close();
      }
    }
    self.function_parents.retain(|(held, _)| held != target);
  }

  /// Clears every watch `target` held, notifies its watchers, and runs
  /// the registered termination callbacks.
  fn finish_termination(&mut self, target: &ActorRef) {
    self.function_watching.retain(|(watcher, _)| watcher != target);
    self.watchers.retain(|(watcher, _)| watcher != target);
    self.watch_messages.retain(|(watcher, _, _)| watcher != target);
    self.pending_terminations.retain(|pending| &pending.watcher != target);
    self.notify_watchers(target, true, false);
    for callback in &self.termination_callbacks {
      callback(target);
    }
  }

  /// Ends one live function reference: closes its boundary, clears the
  /// watches it held, and tells its watchers, as a stop does.
  fn stop_function_ref(&mut self, target: &ActorRef) {
    self.close_function_ref(target);
    self.finish_termination(target);
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
  /// the reference unwatches or stops. A target that is not live is
  /// reported at once, as `watch` does.
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
      watcher.send_message(Message::new(notice), target.clone());
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
    // Pekko's `checkWatchingSame`: a plain watch over a pair that
    // already carries a custom message throws IllegalStateException.
    if self
      .watch_messages
      .iter()
      .any(|(existing, watched, _)| existing == watcher && Self::same_watch_target(watched, target))
    {
      panic!(
        "watching {} with a custom message is already registered; a plain watch on the same target is not allowed",
        target.path()
      );
    }
    if target.is_detached() || self.is_live(target) {
      if !self
        .watchers
        .iter()
        .any(|(existing, watched)| existing == watcher && Self::same_watch_target(watched, target))
      {
        self.watchers.push((watcher.clone(), target.clone()));
      }
    } else {
      self.enqueue_termination(watcher.clone(), Terminated::new(target.clone(), false, false), None);
    }
  }

  /// Registers `watcher` for `target` with a custom notice message,
  /// as `ActorContext.watchWith` does. One pair carries one notice:
  /// a different message for a pair that is already registered, in
  /// either direction, panics, as Pekko's `checkWatchingSame` throws.
  /// Repeating the same registration is idempotent; message equality
  /// is by payload type, the strongest comparison the erased
  /// `Message` offers.
  pub(crate) fn watch_with(&mut self, watcher: &ActorRef, target: &ActorRef, message: Message) {
    let existing = self
      .watch_messages
      .iter()
      .find(|(existing, watched, _)| existing == watcher && Self::same_watch_target(watched, target));
    match existing {
      | Some((_, _, registered)) => {
        if registered.payload_type_id() != message.payload_type_id() {
          panic!("watching {} with a different custom message is not allowed", target.path());
        }
        return;
      },
      | None => {
        if self
          .watchers
          .iter()
          .any(|(existing, watched)| existing == watcher && Self::same_watch_target(watched, target))
        {
          panic!("watching {} with a custom message is not allowed after a plain watch", target.path());
        }
      },
    }
    self.watch_messages.push((watcher.clone(), target.clone(), message.clone()));
    if target.is_detached() || self.is_live(target) {
      self.watchers.push((watcher.clone(), target.clone()));
    } else {
      self.enqueue_termination(watcher.clone(), Terminated::new(target.clone(), false, false), Some(message));
    }
  }

  /// Returns whether `watcher` watches `target`, in either table.
  pub(crate) fn is_watching(&self, watcher: &ActorRef, target: &ActorRef) -> bool {
    self.watchers.iter().any(|(existing, watched)| existing == watcher && Self::same_watch_target(watched, target))
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
      .watch_messages
      .retain(|(existing, watched, _)| existing != watcher || !Self::same_watch_target(watched, target));
    self
      .pending_terminations
      .retain(|pending| &pending.watcher != watcher || !Self::same_watch_target(pending.notice.target(), target));
  }

  /// Runs every mailbox that has work, and delivers every envelope that is
  /// waiting at the boundary, until nothing is left. A mailbox runs up to its
  /// throughput at a time (design of #234, section 3.9).
  pub(crate) fn run_until_idle(&mut self) {
    loop {
      self.take_one_from_boundary();
      let Some(task) = self.manual.pop() else {
        return;
      };
      self.run_task(task, None);
    }
  }

  /// Hands at most one normal message to an actor, or delivers at most one
  /// envelope from the boundary, and returns whether it did.
  ///
  /// It first takes at most one envelope from the boundary and puts it behind
  /// the work that waits, so a boundary that never runs dry does not starve
  /// the mailboxes. Then it takes work in order: a mailbox runs with a
  /// throughput of one, and a run that handles only system messages, or finds
  /// nothing to do, does not count, so the next piece of work runs.
  pub(crate) fn run_one(&mut self) -> bool {
    self.take_one_from_boundary();
    while let Some(task) = self.manual.pop() {
      if self.run_task(task, Some(Throughput::new(1))) {
        return true;
      }
    }
    false
  }

  fn take_one_from_boundary(&mut self) {
    if let Some(envelope) = self.boundary.poll() {
      self.manual.submit(ManualTask::Envelope(envelope));
    }
  }

  /// Runs one piece of work and returns whether it handed a normal message to
  /// an actor or delivered an envelope.
  fn run_task(&mut self, task: ManualTask<ActorCell>, budget: Option<Throughput>) -> bool {
    // A panic out of an envelope's delivery (a function reference's
    // handler, a hook it reaches) is fatal as one out of a run is.
    self.in_run = true;
    let delivered = match task {
      | ManualTask::Envelope(envelope) => {
        self.deliver(envelope, budget);
        true
      },
      | ManualTask::Run(run) => self.run_mailbox(run, budget),
    };
    self.in_run = false;
    delivered
  }

  /// Runs one mailbox, then does what the run left. Returns whether the run
  /// handed a normal message to the actor.
  fn run_mailbox(&mut self, run: MailboxRun<ActorCell>, budget: Option<Throughput>) -> bool {
    let report = run.run(self, budget);
    if let MailboxOutcome::Again(again) = report.outcome {
      self.manual.submit(ManualTask::Run(again));
    }
    self.finish_run();
    report.delivered > 0
  }

  /// Does what a run left, now that the actor is back in its mailbox
  /// (design of #234, section 11.11.2): the failure, `Kill`, or stop it
  /// recorded, a `Resume` for an actor that a failure suspended and that
  /// is still there, and the work at the boundary.
  fn finish_run(&mut self) {
    if let Some((key, after)) = self.after_run.take() {
      let target = self.actors.get(&key).map(|entry| entry.self_ref.clone()).expect("a run's actor has an entry");
      let suspended = match after {
        | AfterRun::Kill => {
          self.handle_kill(&target);
          true
        },
        | AfterRun::Failed { message, sender } => {
          let cell = self.take_cell(&target).expect("the run has put the cell back (invariant I1)");
          self.apply_failure(cell, &target, RUNTIME_CAUSE, &message, Some(sender));
          true
        },
        | AfterRun::Stop { sender } => {
          self.stop_with_sender(&target, sender);
          false
        },
        | AfterRun::PoisonPill => {
          self.stop_at_boundary(&target);
          false
        },
      };
      if suspended {
        self.resume_if_open(&target);
      }
    }
    self.finish_boundary();
  }

  /// Sends a `Resume` to `target` when its mailbox is still open: the parent
  /// has decided for the failure that suspended it.
  fn resume_if_open(&self, target: &ActorRef) {
    if let Some(entry) = self.entry(target) {
      if !entry.mailbox.is_closed() {
        let _ = Mailbox::system_send(&entry.mailbox, SystemMessage::Resume);
      }
    }
  }

  /// Hands one normal message to the actor of `cell` (Pekko's
  /// `ActorCell.invoke`), and says whether its run may take the next.
  ///
  /// A `Kill`, a `PoisonPill`, a failure under the catcher, a stop the actor
  /// asked for after a reply, and work left at the boundary all need the
  /// actor back in its mailbox, so the run yields and the executor does them
  /// once the run has returned. A failure, and a `Kill`, suspend the mailbox
  /// until the parent has decided, as Pekko's `handleInvokeFailure` does.
  fn invoke(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>, envelope: Envelope) -> Next {
    let (message, sender) = envelope.into_message();
    let message = match message.downcast_ref::<Arc<PendingTermination>>() {
      | Some(pending) => match self.take_pending_termination(pending) {
        | Some(message) => message,
        | None => return Next::Continue,
      },
      | None => message,
    };
    let key = cell.key();
    if message == KILL {
      mailbox.suspend();
      self.after_run = Some((key, AfterRun::Kill));
      return Next::Yield;
    }
    if message == POISON_PILL {
      self.after_run = Some((key, AfterRun::PoisonPill));
      return Next::Yield;
    }
    let catcher = self.catcher.clone();
    self.running.push(key);
    let (outcome, sender) = cell.receive(self, message, sender, catcher.as_deref());
    self.exit_frame();
    let deferred_stop = self.actors.get(&key).is_some_and(|entry| !entry.deferred_replies.is_empty());
    match outcome {
      | RunOutcome::Completed if deferred_stop => {
        self.after_run = Some((key, AfterRun::Stop { sender: None }));
        Next::Yield
      },
      | RunOutcome::Completed => {
        if self.pending_stops.is_empty()
          && self.pending_decider_failures.is_empty()
          && self.pending_child_terminated.is_empty()
        {
          Next::Continue
        } else {
          Next::Yield
        }
      },
      | RunOutcome::Failed { message } => {
        mailbox.suspend();
        let after =
          if deferred_stop { AfterRun::Stop { sender: Some(sender) } } else { AfterRun::Failed { message, sender } };
        self.after_run = Some((key, after));
        Next::Yield
      },
    }
  }

  /// Consumes a queued DeathWatch notice and returns what the watcher
  /// receives, or `None` when an `unwatch` cancelled it.
  fn take_pending_termination(&mut self, pending: &Arc<PendingTermination>) -> Option<Message> {
    let index = self.pending_terminations.iter().position(|queued| Arc::ptr_eq(queued, pending))?;
    self.pending_terminations.swap_remove(index);
    Some(match &pending.custom {
      | Some(custom) => custom.clone(),
      | None => Message::new(pending.notice.clone()),
    })
  }

  /// Waits at most `max` for work: a mailbox to run, an envelope at the
  /// boundary, or a message that reached a function reference. Returns
  /// whether work came.
  ///
  /// With a boundary that hands out a [`WakeSignal`](crate::kernel::dispatch::WakeSignal),
  /// the wait ends as soon as work comes. A signal raised for work that is
  /// already done does not end it: the wait goes on until `max` (design of
  /// #234, section 11.11.6). A boundary without a signal is polled for `max`,
  /// and what it gives waits for the next run.
  pub(crate) fn await_envelope(&mut self, max: Duration) -> bool {
    let arrived = self.arrivals.as_ref().map_or(0, |arrivals| arrivals.count());
    let has_work = |executor: &Self| {
      !executor.manual.is_empty()
        || executor.boundary.has_pending()
        || executor.arrivals.as_ref().is_some_and(|arrivals| arrivals.count() != arrived)
    };
    if has_work(self) {
      return true;
    }
    let Some(arrivals) = self.arrivals.clone() else {
      // Without a signal, a send to a mailbox from another thread wakes
      // nothing: the boundary is polled in short slices, and the mailboxes
      // are looked at between them.
      let mut waited = Duration::ZERO;
      while waited < max {
        let slice = (max - waited).min(POLL_SLICE);
        if let Some(envelope) = self.boundary.poll_within(slice) {
          self.manual.submit(ManualTask::Envelope(envelope));
          return true;
        }
        waited += slice;
        if has_work(self) {
          return true;
        }
      }
      return false;
    };
    let Some(start) = self.boundary.monotonic_time() else {
      // Without a clock, a wait that a stale signal ends is started again
      // for `max`; only a wait that sees no signal at all ends with `false`.
      while arrivals.wait(max) {
        if has_work(self) {
          return true;
        }
      }
      return has_work(self);
    };
    let deadline = start.saturating_add(max);
    loop {
      let now = self.boundary.monotonic_time().unwrap_or(deadline);
      if now >= deadline {
        return has_work(self);
      }
      arrivals.wait(deadline - now);
      if has_work(self) {
        return true;
      }
    }
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
    self.routes_closed.store(true, Ordering::SeqCst);
    self.fatal = false;
    self.boundary.close();
    for reference in &self.function_refs {
      if let Some(boundary) = reference.boundary() {
        boundary.close();
      }
    }
    self.function_refs.clear();
    self.function_parents.clear();
    self.function_watching.clear();
    // Every mailbox closes before anything is dropped, so that an actor whose
    // `Drop` sends finds its peers closed. The cells, the messages they held,
    // and the work that waited are dropped after that, outside every lock.
    let entries = core::mem::take(&mut self.actors);
    let mut cells = Vec::with_capacity(entries.len());
    let mut held = Vec::with_capacity(entries.len());
    for entry in entries.values() {
      held.push(entry.mailbox.close());
      cells.push(entry.mailbox.try_take_cell());
    }
    let tasks = self.manual.close();
    drop(cells);
    drop(held);
    drop(tasks);
    drop(entries);
    self.ref_parents.clear();
    self.live.clear();
    self.after_run = None;
    self.in_run = false;
    self.watchers.clear();
    self.watch_messages.clear();
    self.pending_terminations.clear();
    self.user_children.clear();
    self.pending_stops.clear();
    self.pending_decider_failures.clear();
    self.pending_child_terminated.clear();
    self.running.clear();
    self.termination_callbacks.clear();
    self.boundary_active = false;
    while self.boundary.poll().is_some() {}
  }

  /// Records that a caller's thread unwound out of the executor: a panic that
  /// no catcher caught, which Pekko treats as fatal (design of #234, section
  /// 11.11.2). The frames that were running are forgotten without running
  /// user code, and the next call into the executor terminates the system.
  fn mark_fatal(&mut self) {
    self.fatal = true;
    self.running.clear();
    self.boundary_active = false;
    self.in_run = false;
  }

  /// Lends the executor behind `executor` to `work`, the one way the system
  /// and its weak handle reach it.
  ///
  /// When the call before unwound out of the executor while it was running
  /// an actor (a panic that no catcher caught), the system is terminated
  /// first, as Pekko's uncaught exception handler terminates the system on a
  /// fatal error. When `work` unwinds in the middle of a frame, a run, or the
  /// work at the boundary, the executor is marked so that the next call does
  /// that (design of #234, section 11.11.2). A refusal the executor raises
  /// before it changes anything, as Pekko throws to the caller, leaves the
  /// system as it was.
  pub(crate) fn lend<R>(executor: &spin::Mutex<Executor>, work: impl FnOnce(&mut Executor) -> R) -> R {
    let mut guard = lock(executor);
    if guard.fatal {
      guard.stop_all();
    }
    let mut lent = Lent { executor: &mut guard, returned: false };
    let result = work(lent.executor);
    lent.returned = true;
    result
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
      self.is_local(old) && self.is_actor_terminated(old),
      "expected a stopped actor on this shard: {}",
      old.path()
    );
    old.detached_noop()
  }

  /// Returns the identity of the shard boundary this executor posts to.
  pub(crate) fn boundary_key(&self) -> usize {
    self.manual.key()
  }

  /// Refuses to start an actor once the system has been terminated.
  fn assert_running(&self) {
    assert!(!self.terminated, "{TERMINATED_MESSAGE}");
  }

  /// Delivers the message a selection envelope carries, resolving its
  /// path now.
  ///
  /// Mirrors `ActorSelection.deliverSelection`: a path that names a live
  /// actor or a function reference of this shard is delivered to it; any
  /// other path is routed by the dead letter rules with an empty reference
  /// for that path as the recipient, and a suppressed payload names that
  /// empty reference as the reference that processed it.
  fn deliver_selection(&mut self, path: &ActorPath, message: Message, sender: ActorRef) {
    if let Some(reference) = self.function_refs.iter().find(|reference| reference.has_path(path)) {
      let target = reference.clone();
      self.send(sender, &target, message);
      return;
    }
    if let Some(guardian) = self.guardian_for_path(path) {
      let dead_letters = self.dead_letters.clone();
      self.publish_unhandled(message, dead_letters, guardian);
      return;
    }
    let empty = next_actor_ref(path.clone(), self.boundary.clone());
    self.dead_letters_shared.route_unresolved(message, sender, empty);
  }

  /// Returns whether `target` can still be watched for its end.
  ///
  /// The cell table does not hold the actor that is currently executing it, so
  /// a frame that is running counts as live as well; otherwise an actor would
  /// see itself as terminated from inside `receive`, `pre_start`,
  /// `post_restart`, or a child-terminated hook.
  fn is_live(&self, target: &ActorRef) -> bool {
    self.has_cell(target) || self.function_refs.contains(target) || self.is_running(target)
  }

  /// Returns the handle of the live actor at `path`, counting the one that
  /// runs a frame.
  fn live_actor_at(&self, path: &ActorPath) -> Option<ActorRef> {
    self
      .actors
      .values()
      .find(|entry| entry.self_ref.has_path(path) && (entry.mailbox.has_cell() || self.is_running(&entry.self_ref)))
      .map(|entry| entry.self_ref.clone())
  }

  /// Returns whether `target` is one of the system's guardians. A
  /// guardian is a handle without a cell, so `is_live` never holds for
  /// it; as a parent it is live for as long as the system runs, which
  /// the caller has already established when it asks.
  fn is_guardian(&self, target: &ActorRef) -> bool {
    target == &self.root_guardian || target == &self.user_guardian || target == &self.system_guardian
  }

  /// Sends `message` to `target`, naming `sender` as the sender of the
  /// envelope: onto the mailbox of an actor, or through the boundary of a
  /// handle without one.
  pub(crate) fn send(&mut self, sender: ActorRef, target: &ActorRef, message: Message) {
    target.send_message(message, sender);
  }

  /// Delivers one envelope that came across the boundary, or that a mailbox
  /// turned away.
  ///
  /// The notices the executor posts to itself (the dead letter listener's
  /// stop, a selection to resolve) are carried out here. An envelope for a
  /// live actor goes onto its mailbox. Any other envelope finds no actor: a
  /// guardian reports it as unhandled, and the dead letter rules take the
  /// rest, as a Pekko mailbox hands a message for a stopped actor to the dead
  /// letters provider.
  fn deliver(&mut self, envelope: Envelope, budget: Option<Throughput>) {
    let (recipient, message, sender) = envelope.into_parts();
    let recipient = recipient.expect("an envelope outside a mailbox names its recipient");
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
    if let Some(entry) = self.entry(&recipient).filter(|entry| entry.mailbox.has_cell()) {
      // A boundary that carries a message for an actor hands it to the
      // actor's mailbox, and the mailbox runs in this same step, as the
      // message would have been delivered before.
      let mailbox = &entry.mailbox;
      match mailbox.enqueue(Envelope::without_recipient(message, sender)) {
        | Ok(()) => {
          if let Some(run) = Mailbox::try_schedule(mailbox) {
            self.run_mailbox(run, budget);
          }
        },
        | Err(refused) => {
          let (message, sender) = refused.into_message();
          let dead_letters = self.dead_letters.clone();
          self.dead_letters_shared.route_undeliverable(message, sender, recipient, &dead_letters);
          self.finish_boundary();
        },
      }
      return;
    }
    let message = match message.downcast_ref::<Arc<PendingTermination>>() {
      | Some(pending) => match self.take_pending_termination(pending) {
        | Some(message) => message,
        | None => return,
      },
      | None => message,
    };
    // A guardian has no cell: a message addressed to one is reported
    // as unhandled, with the dead letters as its sender, as Pekko's
    // guardians do for a message they will not act upon.
    if recipient == self.root_guardian || recipient == self.user_guardian || recipient == self.system_guardian {
      let dead_letters = self.dead_letters.clone();
      self.publish_unhandled(message, dead_letters, recipient);
      self.finish_boundary();
      return;
    }
    let dead_letters = self.dead_letters.clone();
    self.dead_letters_shared.route_undeliverable(message, sender, recipient, &dead_letters);
    // The routing publishes synchronously, so the dead letter listener
    // may have reached its limit inside this delivery; the boundary
    // work (including the listener's own stop) runs here as it does at
    // the end of a delivered message.
    self.finish_boundary();
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
    mut cell: Box<ActorCell>,
    target: &ActorRef,
    cause_name: &str,
    message: &str,
    original_sender: Option<ActorRef>,
  ) {
    self.publish_child_failure(target, cause_name, message);
    let strategy = self.effective_strategy(&cell);
    if strategy.is_all_for_one() {
      self.put_cell(cell);
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
          self.put_cell(cell);
          self.stop_with_sender(target, original_sender);
        }
      },
      | Ok(Directive::Stop) => {
        self.put_cell(cell);
        self.stop_with_sender(target, original_sender);
      },
      | Err(message) => {
        self.put_cell(cell);
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

  /// Records a restart request on the cell of `target` and answers whether the
  /// restart is permitted, as [`Executor::grant_restart`] does for a taken cell.
  fn grant_restart_of(&mut self, target: &ActorRef) -> bool {
    let strategy = self.strategy_of(target);
    match strategy.restart_permission() {
      | Some((limit, window)) => {
        let mut history = self.with_cell(target, ActorCell::restart_history).expect("the cell is in its mailbox");
        let allowed = self.apply_permission_to(&mut history, limit, window);
        self.with_cell_mut(target, |cell| cell.set_restart_history(history));
        allowed
      },
      | None => {
        if strategy.records_plain_restart() {
          self.with_cell_mut(target, ActorCell::record_restart_request);
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
    let Some(parent) = self.with_cell(target, ActorCell::parent).flatten() else {
      return false;
    };
    let siblings = self.children_of(&parent);
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
          let Some(mut history) = self.with_cell(sibling, ActorCell::restart_history) else {
            permitted = false;
            break;
          };
          let allowed = history.request_permission(limit, window, now);
          self.with_cell_mut(sibling, |cell| cell.set_restart_history(history));
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
        let Some(cell) = self.take_cell(&sibling) else {
          continue;
        };
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
    if self.is_running(&supervisor) {
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
    let Some(cell) = self.take_cell(supervisor) else {
      self.publish_child_failure(supervisor, RUNTIME_CAUSE, message);
      return;
    };
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
    mut cell: Box<ActorCell>,
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
      let mut context = ActorContext::new(target, hook_sender, self, &mut old_stack);
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
      self.cells_where(|child| child.is_child_of(target) && child.is_created())
    } else {
      Vec::new()
    };
    let catcher = self.catcher.clone();
    let mut stack = BehaviorStack::new();
    self.enter_frame(target);
    let restarted = {
      let mut context = ActorContext::new(target, target.clone(), self, &mut stack);
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
        self.put_cell(cell);
        // A retained child's decider can fail again; defer that failure until
        // this recovery returns instead of restarting the same parent inside it.
        self.enter_frame(target);
        for child in survivors {
          if self.pending_stops.contains(target) || self.pending_stops.contains(&child) {
            continue;
          }
          if let Some(cell) = self.take_cell(&child) {
            self.restart_cell(&child, cell, failure, cause_name, message, None);
          }
        }
        let not_created = |cell: &ActorCell| !cell.is_created();
        for child in self.cells_where(|child| child.is_child_of(target) && !child.is_created()) {
          if self.with_cell(&child, not_created) != Some(true) {
            continue;
          }
          self.stop_children_of(&child);
          if self.with_cell(&child, not_created) == Some(true) {
            self.create_initial(&child);
          }
        }
        self.exit_frame();
      },
      | RunOutcome::Failed { message } => {
        self.publish_child_failure(target, RUNTIME_CAUSE, &message);
        cell.suppress_post_stop();
        self.put_cell(cell);
        self.stop(target);
      },
    }
    self.finish_boundary();
  }

  /// Handles a `Kill`: the instance fails with `ActorKilledException` and the
  /// supervisor decides for a killed child, which the default decider maps to
  /// `Stop`.
  fn handle_kill(&mut self, recipient: &ActorRef) {
    self.publish_child_failure(recipient, ACTOR_KILLED_CAUSE, ACTOR_KILLED_MESSAGE);
    let mut cell = self.take_cell(recipient).expect("the run has put the cell back (invariant I1)");
    let strategy = self.effective_strategy(&cell);
    if strategy.is_all_for_one() {
      self.put_cell(cell);
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
          self.put_cell(cell);
          self.stop(recipient);
        }
      },
      | Ok(Directive::Stop) => {
        self.put_cell(cell);
        self.stop(recipient);
      },
      | Err(message) => {
        self.put_cell(cell);
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
    // Function references that watch through the dedicated table receive
    // the notice as a message whose sender is the ended target itself,
    // and their registration survives the notice until they unwatch or
    // stop. A watcher registered in both tables is told once, through
    // this table, so the notice keeps the ended target as its sender.
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
    let custom: Vec<(ActorRef, Message)> = self
      .watch_messages
      .iter()
      .filter(|(_, watched, _)| Self::same_watch_target(watched, target))
      .map(|(watcher, _, message)| (watcher.clone(), message.clone()))
      .collect();
    self.watch_messages.retain(|(_, watched, _)| !Self::same_watch_target(watched, target));
    for watcher in &watchers {
      let message =
        custom.iter().find(|(custom_watcher, _)| custom_watcher == watcher).map(|(_, message)| message.clone());
      let notice = Terminated::new(target.clone(), existence_confirmed, address_terminated);
      self.enqueue_termination(watcher.clone(), notice, message);
    }
    for watcher in function_watchers {
      let notice = Terminated::new(target.clone(), existence_confirmed, address_terminated);
      watcher.send_message(Message::new(notice), target.clone());
    }
  }

  /// The original and a detached reference can compare equal while addressing
  /// different delivery boundaries.
  fn same_watch_target(watched: &ActorRef, target: &ActorRef) -> bool {
    watched == target && watched.shares_endpoint_with(target)
  }

  /// Enqueues a DeathWatch notice (plain `Terminated` or custom
  /// message), cancellable until it is delivered locally.
  fn enqueue_termination(&mut self, recipient: ActorRef, notice: Terminated, custom: Option<Message>) {
    let sender = recipient.clone();
    if self.is_local(&recipient) {
      if recipient.mailbox_is_closed() {
        // The watcher has already stopped, so its mailbox turns the notice
        // away. Route the notice itself to dead letters here, as the
        // executor's delivery of a refused envelope did before the routing
        // moved to the sender's thread: the pending record is consumed
        // (never registered) and the internal carrier never surfaces as a
        // dead letter's payload (review of PR #295).
        let message = custom.unwrap_or_else(|| Message::new(notice));
        let dead_letters = self.dead_letters.clone();
        self.dead_letters_shared.route_undeliverable(message, sender, recipient, &dead_letters);
        return;
      }
      let pending = Arc::new(PendingTermination { watcher: recipient.clone(), notice, custom });
      self.pending_terminations.push(pending.clone());
      recipient.send_message(Message::new(pending), sender);
    } else {
      let message = custom.unwrap_or_else(|| Message::new(notice));
      recipient.send_message(message, sender);
    }
  }
}

// Marks the executor fatal when the work it was lent to unwinds.
struct Lent<'a> {
  executor: &'a mut Executor,
  returned: bool,
}

impl Drop for Lent<'_> {
  fn drop(&mut self) {
    let executor = &mut *self.executor;
    if !self.returned && (!executor.running.is_empty() || executor.boundary_active || executor.in_run) {
      executor.mark_fatal();
    }
  }
}

impl MailboxCell for ActorCell {
  type Env = Executor;

  fn invoke(&mut self, executor: &mut Executor, mailbox: &Mailbox<Self>, envelope: Envelope) -> Next {
    executor.invoke(self, mailbox, envelope)
  }

  /// In stage 3 the executor sends one system message, the `Resume` after a
  /// failure; a parent's suspend of its children waits for stage 4.
  fn system_invoke(&mut self, _executor: &mut Executor, mailbox: &Mailbox<Self>, message: SystemMessage) {
    match message {
      | SystemMessage::Suspend => {
        mailbox.suspend();
      },
      | SystemMessage::Resume => {
        mailbox.resume();
      },
      | other => unreachable!("nothing sends {other:?} to an actor before stage 4 of #234"),
    }
  }
}
