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

use alloc::{
  boxed::Box,
  collections::{BTreeMap, BTreeSet, VecDeque},
  string::{String, ToString},
  vec::Vec,
};
use core::time::Duration;

use portable_atomic::{AtomicBool, AtomicU64, Ordering};

#[cfg(feature = "test-support")]
use crate::kernel::actor::test_support::ActorControl;
#[cfg(feature = "test-support")]
use crate::kernel::actor::test_support::ActorHandle;
use crate::kernel::{
  actor::{
    dispatch_settings::{CapacityLimit, DispatchSettings},
    ActorContext, ActorCreationError, ActorSelection, ActorSelectionMessage, AnyDeadLetter, BehaviorStack,
    ConfigurationException, DeadLetter, DeadLetterSettings, DeadLetterTrampoline, DeadLettersShared, FsmTimerMark,
    FsmTimerMode, FunctionRef, Identify, InvalidActorNameException,
    Logging::{Error, EventSink, Info, Warning},
    Props, ReceiveTimeout, ReceiveTimeoutSetting, ReceiveTimeoutState, SharedTrampoline, StashOverflowException,
    Terminated, UnhandledMessage,
  },
  actor_path::ActorPath,
  actor_ref::{next_actor_id, next_actor_ref, ActorRef},
  dispatch::{
    DispatcherKind, Dispatchers, Failure, Mailbox, MailboxCell, MailboxRun, ManualExecutor, ManualTask, Next,
    RestartCause, RunLimits, RunOutcome as MailboxOutcome, SystemMessage, Throughput, UnboundedQueue,
    DEFAULT_DISPATCHER_ID,
  },
  event::{DeadLetterListener, EventStream},
  executor::{ActorCell, ArrivalNotice, FunctionBoundary, LiveActors, ShardBoundary},
  message::{Envelope, Message},
  supervision::{Directive, FailureCatcher, FailureKind, RestartHistory, RestartLimit, RunOutcome, SupervisorStrategy},
  sync::{lock, Arc, SpinLockFactory, Weak},
  Clock,
};

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

/// What one actor of the system is, in one place.
///
/// The cell itself lives in the mailbox, which a run takes it out of. What
/// the executor reads and writes while the actor handles a message (the
/// stash, the replies it deferred, what it holds as a parent) stays here,
/// with the executor, until a context can borrow the cell it runs in
/// (stage 4).
struct ActorEntry {
  self_ref: ActorRef,
  parent: Option<ActorRef>,
  mailbox: Arc<Mailbox<ActorCell>>,
  stash: Vec<(u64, Envelope)>,
  capacities: (CapacityLimit, CapacityLimit),
  supervision: Option<ParentEntry>,
  deferred_replies: Vec<DeferredReply>,
  /// The child whose failure this actor's recorded failure answers for,
  /// or the actor itself when the recorded failure is its own (Pekko's
  /// `perpetrator`, which `isFailed` keeps until the answer takes
  /// effect). Empty while no failure waits for an answer.
  perpetrator: Option<ActorRef>,
  /// The system messages set aside while the actor is suspended or waits
  /// for its children, in the order they came.
  system_stash: VecDeque<SystemMessage>,
  /// The restarts this actor has used, read by the decision of a parent
  /// that keeps no table for it (a guardian deciding on the spot). A
  /// child of an actor parent, or of a function reference, keeps its
  /// count in that parent's table instead, and this stays untouched.
  restart_history: RestartHistory,
}

/// What a parent waits for its dying children for (Pekko's
/// `ChildrenContainer.SuspendReason` without `UserRequest`: a child that a
/// user stops leaves nothing to go on with once it has gone).
#[derive(Clone)]
enum WaitReason {
  /// A start that failed is tried again once the children of the failed
  /// attempt have gone (Pekko's `faultCreate`).
  Creation,
  /// The second half of a restart runs once the children that the first half
  /// stopped have gone (Pekko's `faultRecreate`).
  Recreation(RestartCause),
  /// The parent itself stops, and finishes once every child has gone
  /// (Pekko's `terminate`).
  Termination,
}

/// One parent and the facts the executor keeps on its behalf.
///
/// Mirrors the `Children` container that a Pekko `ActorCell` holds for one
/// actor: the strategy the parent applies to its children, every child from
/// its creation until the parent has handled its `ChildTerminated`, the
/// children asked to stop, and what the parent waits for them for. The
/// executor keeps this apart from the parent's cell because the cell is out of
/// the table while the parent handles a message, yet the parent may still ask
/// for a child inside that delivery.
struct ParentEntry {
  parent: ActorRef,
  strategy: SupervisorStrategy,
  /// Every child, named or not, from its creation until the parent has
  /// handled its `ChildTerminated` (Pekko's `ChildrenContainer`).
  children: Vec<ChildRecord>,
  /// The children asked to stop that have not ended yet (Pekko's
  /// `shallDie`).
  dying: Vec<ActorRef>,
  /// What the parent waits for its dying children for (Pekko's
  /// `TerminatingChildrenContainer`).
  waiting: Option<WaitReason>,
  /// The children a restart that is under way keeps, taken when the
  /// restart's first half begins: the second half restarts each one that
  /// is still in the table, together with the children the wait itself
  /// brought in. Empty outside a restart.
  pending_survivors: Option<Vec<ActorRef>>,
}

/// What a parent's table keeps of one child (Pekko's `ChildRestartStats`
/// together with the name and the strategy the child was started under).
#[derive(Clone)]
struct ChildRecord {
  /// The name the child was reserved under.
  name: String,
  /// The child's handle, with its identifier and generation.
  child: ActorRef,
  /// The restarts the child has used, which the parent's decision reads
  /// and a granted restart adds to.
  history: RestartHistory,
  /// The strategy the child was started under, or `None` when it
  /// inherits the parent's.
  spawn_strategy: Option<SupervisorStrategy>,
}

impl ParentEntry {
  fn new(parent: ActorRef, strategy: SupervisorStrategy) -> Self {
    Self { parent, strategy, children: Vec::new(), dying: Vec::new(), waiting: None, pending_survivors: None }
  }

  /// Returns whether `child` is in the table, by the identifier and the
  /// generation of its handle (Pekko's `getChildByRef`).
  fn holds(&self, child: &ActorRef) -> bool {
    self.children.iter().any(|record| &record.child == child)
  }

  /// Returns the record of `child`, when the table holds it.
  fn child_record(&self, child: &ActorRef) -> Option<&ChildRecord> {
    self.children.iter().find(|record| &record.child == child)
  }

  /// Returns the record of `child` to change, when the table holds it.
  fn child_record_mut(&mut self, child: &ActorRef) -> Option<&mut ChildRecord> {
    self.children.iter_mut().find(|record| &record.child == child)
  }

  /// Returns whether the parent stops (Pekko's `isTerminating`).
  fn is_terminating(&self) -> bool {
    matches!(self.waiting, Some(WaitReason::Termination))
  }

  /// Counts `child` among the children that are to die, when the table holds
  /// it (Pekko's `Children.stop`, which calls `shallDie` for a child only).
  fn mark_dying(&mut self, child: &ActorRef) {
    if self.holds(child) && !self.dying.contains(child) {
      self.dying.push(child.clone());
    }
  }

  /// Asks every child to die and waits for them for `reason`. A parent that
  /// stops is marked stopping even without children. Returns the children.
  fn wait_for_all_children(&mut self, reason: WaitReason) -> Vec<ActorRef> {
    let children: Vec<ActorRef> = self.children.iter().map(|record| record.child.clone()).collect();
    if matches!(reason, WaitReason::Termination) || !children.is_empty() {
      self.dying = children.clone();
      self.waiting = Some(reason);
    }
    children
  }

  /// Takes `child` out of the table once it has ended (Pekko's
  /// `removeChildAndGetStateChange`). Returns `None` when the table does not
  /// hold that handle, which leaves the table as it was. Otherwise returns
  /// what the parent waited for when `child` was the last dying child: the
  /// wait ends there, except a stop, which stays marked while it finishes.
  fn remove_child(&mut self, child: &ActorRef) -> Option<Option<WaitReason>> {
    let index = self.children.iter().position(|record| &record.child == child)?;
    self.children.remove(index);
    self.dying.retain(|held| held != child);
    if !self.dying.is_empty() {
      return Some(None);
    }
    let ended = self.waiting.clone();
    if !matches!(ended, Some(WaitReason::Termination) | None) {
      self.waiting = None;
    }
    Some(ended)
  }
}

/// What a decision for one failure is made from: the strategies that may
/// decide for it, the restarts the failed child has used, and the two
/// facts that say which strategy's decider may speak for it.
struct DecisionInputs {
  /// The strategy the deciding parent applies to its children, when it
  /// keeps a table.
  governing: Option<SupervisorStrategy>,
  /// The strategy the failed child was started under, or `None` when it
  /// inherits the deciding parent's.
  spawn_strategy: Option<SupervisorStrategy>,
  /// The restarts the failed child has already used.
  count: u32,
  /// Whether the failed child's recorded failure answers for one of its
  /// own children (its decider panicked on a child's `Failed`).
  escalation: bool,
  /// Whether the deciding parent itself stands under a parent.
  parent_not_top: bool,
}

/// The outcome of choosing a directive for one failure.
enum Decision {
  /// Apply this directive, which the strategy it names chose; the flag
  /// says the strategy governs the whole set of siblings at once.
  Apply(Directive, SupervisorStrategy, bool),
  /// The chosen decider panicked, with the panic's message: the deciding
  /// actor fails with it.
  DeciderPanicked { message: String },
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
/// `ShardBoundary` carries what has no mailbox. The executor alone owns the actors' entries; a
/// caller only holds `ActorRef` handles.
///
/// The executor also owns whether the system has been terminated. Keeping that
/// one fact here lets every entry that starts a new actor, including the ones
/// that reach the executor through a weak handle, refuse the request with the
/// same cause as Pekko.
/// The receive timeout deadline index: `(deadline, seq)` to the
/// actor and the version of its timeout state. The `seq` is drawn
/// from the counter the FSM timer index shares, so the two kinds of
/// rows stand in one scheduling order.
type ReceiveTimeoutIndex = BTreeMap<(Duration, u64), (ActorKey, u32)>;

/// The reverse of [`ReceiveTimeoutIndex`]: the row an actor stands in.
type ReceiveTimeoutIndexed = BTreeMap<ActorKey, (Duration, u64)>;

pub(crate) struct Executor {
  actors: BTreeMap<ActorKey, ActorEntry>,
  ref_parents: Vec<ParentEntry>,
  boundary: Arc<dyn ShardBoundary>,
  manual: Arc<ManualExecutor<ActorCell>>,
  live: Arc<LiveActors>,
  arrivals: Option<Arc<ArrivalNotice>>,
  in_run: bool,
  fatal: bool,
  root: ActorPath,
  watchers: Vec<(ActorRef, ActorRef)>,
  pending_terminations: Vec<Arc<PendingTermination>>,
  function_refs: Vec<ActorRef>,
  catcher: Option<Arc<dyn FailureCatcher>>,
  event_sink: Arc<spin::Mutex<Option<Arc<dyn EventSink>>>>,
  event_stream: Option<EventStream>,
  running: Vec<ActorKey>,
  termination_callbacks: Vec<TerminationCallback>,
  dead_letters: ActorRef,
  dead_letters_shared: Arc<DeadLettersShared>,
  dead_letter_listener: Option<ActorRef>,
  hook_child_creation_refused: bool,
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
  not_influence_receive_timeout: BTreeSet<core::any::TypeId>,
  self_handle: Option<Weak<spin::Mutex<Executor>>>,
  receive_timeouts: BTreeMap<ActorKey, ReceiveTimeoutState>,
  receive_timeout_index: ReceiveTimeoutIndex,
  receive_timeout_indexed: ReceiveTimeoutIndexed,
  fsm_timers: BTreeMap<(ActorKey, FsmTimerSlot), FsmScheduledTimer>,
  fsm_timer_index: BTreeMap<(Duration, u64), (ActorKey, FsmTimerSlot)>,
  timer_seq: u64,
}

/// The slot of one FSM timer of one actor in the executor's FSM table.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum FsmTimerSlot {
  /// The state timer.
  State,
  /// A named timer.
  Named(String),
}

/// One FSM timer registration, before the executor assigns its
/// deadline and sequence.
pub(crate) struct FsmTimerRequest {
  pub(crate) mode: FsmTimerMode,
  pub(crate) message: Message,
  pub(crate) delay: Duration,
  pub(crate) instance_id: u64,
  pub(crate) generation: u64,
  pub(crate) sender: Option<ActorRef>,
}

/// One FSM timer the executor holds for an actor.
struct FsmScheduledTimer {
  deadline: Duration,
  delay: Duration,
  seq: u64,
  mode: FsmTimerMode,
  message: Message,
  instance_id: u64,
  generation: u64,
  /// The sender of the event that armed this timer, which the mark
  /// arrives from; the state timer's mark arrives from the machine
  /// itself.
  sender: Option<ActorRef>,
}

impl Executor {
  pub(crate) fn new(root: ActorPath, boundary: Arc<dyn ShardBoundary>) -> Self {
    let dead_letters_shared = Arc::new(DeadLettersShared::new());
    dead_letters_shared.set_dead_letter_trampoline(Arc::new(SharedTrampoline::new()));
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
      in_run: false,
      fatal: false,
      root,
      watchers: Vec::new(),
      pending_terminations: Vec::new(),
      function_refs: Vec::new(),
      catcher: None,
      event_sink: Arc::new(spin::Mutex::new(None)),
      event_stream: None,
      running: Vec::new(),
      termination_callbacks: Vec::new(),
      dead_letters,
      dead_letters_shared,
      dead_letter_listener: None,
      hook_child_creation_refused: false,
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
      not_influence_receive_timeout: BTreeSet::new(),
      self_handle: None,
      receive_timeouts: BTreeMap::new(),
      receive_timeout_index: BTreeMap::new(),
      receive_timeout_indexed: BTreeMap::new(),
      fsm_timers: BTreeMap::new(),
      fsm_timer_index: BTreeMap::new(),
      timer_seq: 0,
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

  /// Returns every child in the table of `parent`, in the order they were
  /// created, from its creation until `parent` has handled its
  /// `ChildTerminated` (Pekko's `children`).
  fn children_of(&self, parent: &ActorRef) -> Vec<ActorRef> {
    self
      .parent_entry(parent)
      .map(|entry| entry.children.iter().map(|record| record.child.clone()).collect())
      .unwrap_or_default()
  }

  /// Returns the children of `parent` whose cell is in its mailbox: the ones
  /// that have not closed.
  fn open_children_of(&self, parent: &ActorRef) -> Vec<ActorRef> {
    self.children_of(parent).into_iter().filter(|child| self.has_cell(child)).collect()
  }

  /// Returns whether `child` is in the table of `parent`, by the identifier
  /// and the generation of its handle (Pekko's `getChildByRef`).
  fn is_child_in_table(&self, parent: &ActorRef, child: &ActorRef) -> bool {
    self.parent_entry(parent).is_some_and(|entry| entry.holds(child))
  }

  /// Returns whether `parent` is stopping: it has handled its `Terminate`
  /// (Pekko's `isTerminating`). It refuses every new child.
  fn is_terminating(&self, parent: &ActorRef) -> bool {
    self.parent_entry(parent).is_some_and(ParentEntry::is_terminating)
  }

  /// Returns whether `target` waits for its children, for any reason.
  fn is_waiting(&self, target: &ActorRef) -> bool {
    self.parent_entry(target).is_some_and(|entry| entry.waiting.is_some())
  }

  /// Sends `message` to the mailbox of `target`. A closed mailbox hands
  /// the message back, and it goes to dead letters through
  /// [`route_closed_system_message`](Self::route_closed_system_message);
  /// a target without a mailbox has nowhere to route it from and the
  /// message is dropped.
  fn send_system(&mut self, target: &ActorRef, message: SystemMessage) {
    let mailbox = self.entry(target).map(|entry| Arc::clone(&entry.mailbox));
    if let Some(mailbox) = mailbox {
      if let Err(message) = Mailbox::system_send(&mailbox, message) {
        self.route_closed_system_message(message, target.clone());
      }
    }
  }

  /// Routes a system message whose delivery met a closed mailbox to dead
  /// letters, through the one place that decides how such a message is
  /// reported.
  fn route_closed_system_message(&mut self, message: SystemMessage, recipient: ActorRef) {
    let dead_letters = self.dead_letters.clone();
    self.dead_letters_shared.route_system_undeliverable(message, recipient, &dead_letters);
  }

  /// Installs the trampoline the dead letter routing rules publish
  /// through, replacing the kernel's own trampoline the executor was
  /// created with.
  pub(crate) fn set_dead_letter_trampoline(&self, trampoline: Arc<dyn DeadLetterTrampoline>) {
    self.dead_letters_shared.set_dead_letter_trampoline(trampoline);
  }

  /// Takes one from the suspend count of `target` and reserves its mailbox,
  /// which may now have normal messages to run.
  fn resume_mailbox(&self, target: &ActorRef) {
    if let Some(entry) = self.entry(target) {
      entry.mailbox.resume();
      Mailbox::schedule(&entry.mailbox);
    }
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

  /// Records the weak handle of the system this executor runs, which
  /// the system hands in once it has built the executor's shared cell.
  pub(crate) fn set_self_handle(&mut self, handle: Weak<spin::Mutex<Executor>>) {
    self.self_handle = Some(handle);
  }

  /// Returns a non-owning handle to the system this executor runs.
  ///
  /// # Panics
  ///
  /// Panics when the system never handed its handle in, which no
  /// system built through `ActorSystem` does.
  #[cfg(feature = "test-support")]
  pub(crate) fn weak_system(&self) -> crate::kernel::actor_system::WeakActorSystem {
    let handle = self.self_handle.clone().expect("the actor system handed its handle to its executor");
    crate::kernel::actor_system::WeakActorSystem::new(self.root.root().name().to_string(), handle)
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
  /// the executor's and walks no way back.
  fn dead_letter_route(&self) -> Arc<dyn Fn(Envelope) + Send + Sync> {
    let shared = self.dead_letters_shared.clone();
    let dead_letters = self.dead_letters.clone();
    let routes_closed = self.routes_closed.clone();
    Arc::new(move |envelope: Envelope| {
      // Once the system has terminated, a refused envelope is discarded.
      if routes_closed.load(Ordering::SeqCst) {
        return;
      }
      let (recipient, message, sender) = envelope.into_parts();
      #[cfg(feature = "test-support")]
      if message.is::<ActorControl>() {
        return;
      }
      if let Some(recipient) = recipient {
        shared.route_undeliverable(message, sender, recipient, &dead_letters);
      }
    })
  }

  /// Routes `message` to dead letters through the shared rules, with the
  /// executor's dead letters handle as the destination of the routing.
  fn route_undeliverable(&self, message: Message, sender: ActorRef, recipient: ActorRef) {
    #[cfg(feature = "test-support")]
    if message.is::<ActorControl>() {
      return;
    }
    self.dead_letters_shared.route_undeliverable(message, sender, recipient, &self.dead_letters);
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
    let previous_now = self.now_opt();
    let installed_now = clock.monotonic();
    self.clock = Some(clock);
    let Some(previous_now) = previous_now else {
      return;
    };
    // A deadline is a point on the clock that measured it. Reading the
    // same point on the installed clock would fire the timeout early or
    // late by the distance between the two clocks, so every scheduled
    // deadline keeps the remaining time it had on the previous clock.
    let keys: alloc::vec::Vec<ActorKey> = self.receive_timeout_indexed.keys().copied().collect();
    for key in keys {
      let moved = match self.receive_timeouts.get_mut(&key) {
        | Some(state) => match state.scheduled() {
          | Some((deadline, _)) => {
            let remaining = deadline.saturating_sub(previous_now);
            match installed_now.checked_add(remaining) {
              | Some(new_deadline) => state.move_deadline(new_deadline),
              | None => state.cancel_task(),
            }
            true
          },
          | None => false,
        },
        | None => false,
      };
      if moved {
        self.sync_receive_timeout_index(key);
      }
    }
    // The FSM timers keep the remaining time they had as well: each
    // row's index entry is rewritten under its new deadline with the
    // sequence it already had, and a deadline the installed clock
    // cannot name is cancelled, as a receive timeout is.
    let rows: Vec<((ActorKey, FsmTimerSlot), Duration, u64)> =
      self.fsm_timers.iter().map(|(id, timer)| (id.clone(), timer.deadline, timer.seq)).collect();
    for ((key, slot), deadline, seq) in rows {
      let remaining = deadline.saturating_sub(previous_now);
      match installed_now.checked_add(remaining) {
        | Some(new_deadline) => {
          self.fsm_timer_index.remove(&(deadline, seq));
          if let Some(timer) = self.fsm_timers.get_mut(&(key, slot.clone())) {
            timer.deadline = new_deadline;
          }
          self.fsm_timer_index.insert((new_deadline, seq), (key, slot));
        },
        | None => self.remove_fsm_timer(key, &slot),
      }
    }
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

  /// Returns the monotonic time of the installed clock, or of the
  /// boundary when no clock is installed, or `None` when neither exists.
  fn now_opt(&self) -> Option<Duration> {
    self.clock.as_ref().map(|clock| clock.monotonic()).or_else(|| self.boundary.monotonic_time())
  }

  /// Panics when a finite receive timeout cannot be timed.
  ///
  /// A finite receive timeout is never silently left unfired: without a
  /// monotonic clock there is no way to tell when it comes due, so the
  /// call that sets it fails with this reason instead.
  pub(crate) fn require_receive_timeout_clock(&self) {
    if self.now_opt().is_none() {
      panic!("a finite receive timeout needs a monotonic clock, but this system has none");
    }
  }

  /// Registers a payload type whose messages do not influence the receive
  /// timeout, as `register_dead_letter_suppression` does for its table.
  pub(crate) fn register_not_influence_receive_timeout(&mut self, id: core::any::TypeId) {
    self.not_influence_receive_timeout.insert(id);
  }

  /// Returns whether `message` does not influence the receive timeout:
  /// the built-in `Identify`, or a type registered with
  /// [`ActorSystem::register_not_influence_receive_timeout`](crate::kernel::actor_system::ActorSystem::register_not_influence_receive_timeout).
  ///
  /// Mirrors `ReceiveTimeoutCompat.isNotInfluenceReceiveTimeout`, which
  /// looks at the concrete `Identify` class first and then at the marker.
  pub(crate) fn is_not_influence_receive_timeout(&self, message: &Message) -> bool {
    message.is::<Identify>() || self.not_influence_receive_timeout.contains(&message.payload_type_id())
  }

  /// Records the receive timeout `key`'s actor set last.
  ///
  /// Mirrors `ActorContext.setReceiveTimeout` reaching the cell's
  /// `dungeon.ReceiveTimeout`: the executor keeps the state because the
  /// context only borrows it while a call runs.
  pub(crate) fn set_receive_timeout(&mut self, key: ActorKey, timeout: ReceiveTimeoutSetting) {
    match self.receive_timeouts.get_mut(&key) {
      | Some(state) => state.set_receive_timeout(timeout),
      | None => {
        self.receive_timeouts.insert(key, ReceiveTimeoutState::new(timeout));
      },
    }
    self.sync_receive_timeout_index(key);
  }

  /// Returns the receive timeout `key`'s actor set last, or
  /// [`ReceiveTimeoutSetting::Undefined`] when it never set one.
  pub(crate) fn receive_timeout_setting(&self, key: ActorKey) -> ReceiveTimeoutSetting {
    self.receive_timeouts.get(&key).map_or(ReceiveTimeoutSetting::Undefined, |state| state.setting())
  }

  /// Keeps the deadline index in step with the state of `key`: the old
  /// row is removed at once, and the new one is inserted when a firing is
  /// scheduled, so the index never holds a stale row.
  fn sync_receive_timeout_index(&mut self, key: ActorKey) {
    if let Some((old_deadline, old_seq)) = self.receive_timeout_indexed.remove(&key) {
      self.receive_timeout_index.remove(&(old_deadline, old_seq));
    }
    if let Some(state) = self.receive_timeouts.get(&key) {
      if let Some((deadline, _)) = state.scheduled() {
        self.timer_seq += 1;
        let seq = self.timer_seq;
        self.receive_timeout_index.insert((deadline, seq), (key, state.version()));
        self.receive_timeout_indexed.insert(key, (deadline, seq));
      }
    }
  }

  /// Runs `cancelReceiveTimeoutIfNeeded` for the handling of one message
  /// of `key`'s actor and returns the version from before the handling.
  fn receive_timeout_before(&mut self, key: ActorKey, influences: bool) -> u32 {
    let version = match self.receive_timeouts.get_mut(&key) {
      | Some(state) => {
        if influences {
          state.cancel_task();
        }
        state.version()
      },
      | None => 0,
    };
    self.sync_receive_timeout_index(key);
    version
  }

  /// Runs `checkReceiveTimeoutIfNeeded` after one handling of `key`'s
  /// actor: the check runs when the actor has timeout state, or when the
  /// handling changed it.
  fn receive_timeout_after(&mut self, key: ActorKey, influences: bool, before: u32) {
    let changed = self.receive_timeouts.get(&key).map_or(0, |state| state.version()) != before;
    if self.receive_timeouts.contains_key(&key) || changed {
      self.check_receive_timeout(key, influences || changed);
    }
  }

  /// Runs `checkReceiveTimeout` for `key`'s actor.
  fn check_receive_timeout(&mut self, key: ActorKey, reschedule: bool) {
    let finite =
      matches!(self.receive_timeouts.get(&key).map(|state| state.setting()), Some(ReceiveTimeoutSetting::Finite(_)));
    let now = if finite {
      match self.now_opt() {
        | Some(now) => now,
        // The call that set the timeout already required a clock. A
        // boundary that stops reporting time afterwards leaves the
        // timeout unchecked here instead of panicking: some callers run
        // this check with the actor's cell out of its mailbox, where a
        // panic would lose the cell.
        | None => return,
      }
    } else {
      Duration::ZERO
    };
    if let Some(state) = self.receive_timeouts.get_mut(&key) {
      state.check(reschedule, now);
    }
    self.sync_receive_timeout_index(key);
  }

  /// Drops the receive timeout state of `key`'s actor, as `terminate` and
  /// `faultCreate` cancel the timeout in the original.
  fn clear_receive_timeout(&mut self, key: ActorKey) {
    self.receive_timeouts.remove(&key);
    self.sync_receive_timeout_index(key);
    self.clear_fsm_timers(key);
  }

  /// Drops every FSM timer of `key`'s actor from the deadline index at
  /// once, as `terminate` of the original cancels every timer of the
  /// machine it ends.
  pub(crate) fn clear_fsm_timers(&mut self, key: ActorKey) {
    let slots: Vec<FsmTimerSlot> =
      self.fsm_timers.keys().filter(|(owner, _)| *owner == key).map(|(_, slot)| slot.clone()).collect();
    for slot in slots {
      self.remove_fsm_timer(key, &slot);
    }
  }

  fn remove_fsm_timer(&mut self, key: ActorKey, slot: &FsmTimerSlot) {
    if let Some(old) = self.fsm_timers.remove(&(key, slot.clone())) {
      self.fsm_timer_index.remove(&(old.deadline, old.seq));
    }
  }

  fn insert_fsm_timer(&mut self, key: ActorKey, slot: FsmTimerSlot, request: FsmTimerRequest) {
    let FsmTimerRequest { mode, message, delay, instance_id, generation, sender } = request;
    self.remove_fsm_timer(key, &slot);
    if self.now_opt().is_none() {
      panic!("FSM timer: a finite FSM timer needs a monotonic clock, but this system has none");
    }
    let now = self.now_opt().expect("checked above");
    let Some(deadline) = now.checked_add(delay) else {
      return;
    };
    self.timer_seq += 1;
    let seq = self.timer_seq;
    self.fsm_timers.insert(
      (key, slot.clone()),
      FsmScheduledTimer { deadline, delay, seq, mode, message, instance_id, generation, sender },
    );
    self.fsm_timer_index.insert((deadline, seq), (key, slot));
  }

  /// Registers a named FSM timer of `key`'s actor.
  pub(crate) fn schedule_fsm_named_timer(&mut self, key: ActorKey, name: &str, request: FsmTimerRequest) {
    self.insert_fsm_timer(key, FsmTimerSlot::Named(name.to_string()), request);
  }

  /// Registers the state timer of `key`'s actor.
  pub(crate) fn schedule_fsm_state_timer(&mut self, key: ActorKey, generation: u64, delay: Duration, instance_id: u64) {
    let request = FsmTimerRequest {
      mode: FsmTimerMode::Single,
      message: Message::new(crate::kernel::actor::StateTimeout),
      delay,
      instance_id,
      generation,
      sender: None,
    };
    self.insert_fsm_timer(key, FsmTimerSlot::State, request);
  }

  /// Cancels a named FSM timer of `key`'s actor.
  pub(crate) fn cancel_fsm_named_timer(&mut self, key: ActorKey, name: &str) {
    self.remove_fsm_timer(key, &FsmTimerSlot::Named(name.to_string()));
  }

  /// Cancels the state timer of `key`'s actor.
  pub(crate) fn cancel_fsm_state_timer(&mut self, key: ActorKey) {
    self.remove_fsm_timer(key, &FsmTimerSlot::State);
  }

  /// Returns the earliest scheduled FSM timer deadline.
  fn earliest_fsm_timer_deadline(&self) -> Option<Duration> {
    self.fsm_timer_index.keys().next().map(|(deadline, _)| *deadline)
  }

  /// Returns the earliest deadline of a receive timeout or an FSM timer.
  fn earliest_timer_deadline(&self) -> Option<Duration> {
    match (self.earliest_receive_timeout_deadline(), self.earliest_fsm_timer_deadline()) {
      | (Some(a), Some(b)) => Some(a.min(b)),
      | (Some(a), None) => Some(a),
      | (None, Some(b)) => Some(b),
      | (None, None) => None,
    }
  }

  /// Fires the receive timeouts and the FSM timers that are due, in
  /// one order across both kinds: the row with the smallest
  /// `(deadline, seq)` fires first, where `seq` is the order the rows
  /// were scheduled in, drawn from the one counter both kinds share.
  /// The clock is read once for the whole pass, so both kinds are
  /// judged against the same reading: a receive timeout whose
  /// deadline is later than an FSM timer's cannot be delivered first
  /// and so cannot invalidate the timer's mark by arriving before it.
  fn fire_due_timers(&mut self) {
    if self.receive_timeout_index.is_empty() && self.fsm_timer_index.is_empty() {
      return;
    }
    let Some(now) = self.now_opt() else {
      return;
    };
    loop {
      let receive_next = self.receive_timeout_index.iter().next().map(|(&(deadline, seq), _)| (deadline, seq));
      let fsm_next = self.fsm_timer_index.iter().next().map(|(&(deadline, seq), _)| (deadline, seq));
      let fire_receive = match (receive_next, fsm_next) {
        | (Some(receive), Some(fsm)) => receive <= fsm,
        | (Some(_), None) => true,
        | (None, Some(_)) => false,
        | (None, None) => return,
      };
      let progressed = if fire_receive { self.fire_one_receive_timeout(now) } else { self.fire_one_fsm_timer(now) };
      if !progressed {
        return;
      }
    }
  }

  /// Fires the earliest FSM timer when its deadline has come by
  /// `now`, and returns whether a row was consumed: the deadline is
  /// consumed, a repeating timer is registered again, and the mark
  /// goes to the machine's mailbox. A row registered again in this
  /// pass fires again only when its new deadline is not after the
  /// one reading the pass shares (a fixed-rate timer whose clock
  /// jumped catches up, as the original's does).
  fn fire_one_fsm_timer(&mut self, now: Duration) -> bool {
    let Some((&(deadline, seq), _)) = self.fsm_timer_index.iter().next() else {
      return false;
    };
    if deadline > now {
      return false;
    }
    let Some((key, slot)) = self.fsm_timer_index.remove(&(deadline, seq)) else {
      return false;
    };
    let Some(timer) = self.fsm_timers.remove(&(key, slot.clone())) else {
      return true;
    };
    let message = timer.message.clone();
    let mode = timer.mode;
    let instance_id = timer.instance_id;
    let generation = timer.generation;
    let delay = timer.delay;
    let timer_sender = timer.sender.clone();
    let next_deadline = match mode {
      | FsmTimerMode::Single => None,
      | FsmTimerMode::FixedDelay => now.checked_add(delay),
      | FsmTimerMode::FixedRate => deadline.checked_add(delay),
    };
    if let Some(next) = next_deadline {
      self.timer_seq += 1;
      let new_seq = self.timer_seq;
      self.fsm_timer_index.insert((next, new_seq), (key, slot.clone()));
      self.fsm_timers.insert(
        (key, slot.clone()),
        FsmScheduledTimer {
          deadline: next,
          delay,
          seq: new_seq,
          mode,
          message: message.clone(),
          instance_id,
          generation,
          sender: timer_sender.clone(),
        },
      );
    }
    let Some(entry) = self.actors.get(&key) else {
      return true;
    };
    let target = entry.self_ref.clone();
    if message == POISON_PILL || message == KILL {
      let sender = self.dead_letters();
      self.send(sender, &target, message);
      return true;
    }
    let mark = match &slot {
      | FsmTimerSlot::State => FsmTimerMark::StateTimeout { instance_id, generation },
      | FsmTimerSlot::Named(name) => FsmTimerMark::Named { instance_id, name: name.clone(), generation },
    };
    let sender = timer_sender.unwrap_or_else(|| target.clone());
    self.send(sender, &target, Message::new(mark));
    true
  }

  /// Publishes a warning that names `source` as its source, to the
  /// event stream and to the event sink.
  ///
  /// Mirrors `log.warning(...)` reaching Pekko's `TestEventListener`
  /// through the system's logging: the stream receives the warning as
  /// an event, and the sink, the test-side tap, receives the same one.
  pub(crate) fn publish_warning(&self, source: &ActorRef, message: &str) {
    let warning = Warning::new(source.path().to_string(), ACTOR_LOG_CLASS, message);
    if let Some(stream) = self.event_stream.as_ref() {
      stream.publish(warning.clone());
    }
    let sink = self.event_sink.lock().clone();
    if let Some(sink) = sink {
      sink.publish(&warning);
    }
  }

  /// Returns the live child of `parent` named `name`, if any.
  pub(crate) fn child_of(&self, parent: &ActorRef, name: &str) -> Option<ActorRef> {
    self
      .parent_entry(parent)
      .and_then(|entry| entry.children.iter().find(|record| record.name == name).map(|record| record.child.clone()))
  }

  /// Returns the earliest scheduled receive timeout deadline.
  fn earliest_receive_timeout_deadline(&self) -> Option<Duration> {
    self.receive_timeout_index.keys().next().map(|(deadline, _)| *deadline)
  }

  /// Fires the earliest receive timeout when its deadline has come
  /// by `now`, and returns whether a row was consumed: the deadline
  /// is consumed and one `ReceiveTimeout` goes to the actor's
  /// mailbox, where it is an ordinary message that is always
  /// delivered, however the setting changes afterwards.
  fn fire_one_receive_timeout(&mut self, now: Duration) -> bool {
    let Some((&(deadline, seq), &(key, version))) = self.receive_timeout_index.iter().next() else {
      return false;
    };
    if deadline > now {
      return false;
    }
    self.receive_timeout_index.remove(&(deadline, seq));
    self.receive_timeout_indexed.remove(&key);
    let fired = match self.receive_timeouts.get_mut(&key) {
      | Some(state) => match state.scheduled() {
        | Some((scheduled_deadline, timeout)) if scheduled_deadline == deadline && state.version() == version => {
          state.mark_fired();
          Some(timeout)
        },
        | _ => None,
      },
      | None => None,
    };
    let Some(timeout) = fired else {
      return true;
    };
    let Some(entry) = self.actors.get(&key) else {
      return true;
    };
    let target = entry.self_ref.clone();
    let sender = self.dead_letters();
    self.send(sender, &target, Message::new(ReceiveTimeout::new(timeout)));
    true
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
    #[cfg(feature = "test-support")]
    if message.is::<ActorControl>() {
      // A test's probe is an internal message: it is dropped here
      // instead of being published as a dead letter, so a test that
      // watches dead letters is not disturbed by a probe that arrived
      // after its actor stopped, or that its mailbox still held then.
      return;
    }
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
  /// a message to the head is reported as unhandled. A top-level actor
  /// takes its name in `user_children` when it is created, and frees it
  /// once it has stopped (section 11.12.4).
  pub(crate) fn try_spawn_user(&mut self, props: Props, name: Option<&str>) -> Result<ActorRef, ActorCreationError> {
    if let Some(name) = name {
      validate_actor_name(name)?;
    }
    if self.is_terminated() {
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
    Ok(self.spawn_at(head_props, path, None, SupervisorStrategy::default_strategy(), false))
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
    assert!(!self.is_terminating(parent), "{}", InvalidActorNameException::terminating());
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
    let mailbox =
      Mailbox::for_actor(queue, limits, &SpinLockFactory, Arc::downgrade(&self.manual), self.dead_letter_route());
    let mailbox = Arc::new(mailbox);
    let label = path.name().to_string();
    let self_ref = ActorRef::for_mailbox(next_actor_id(), Arc::new(path), Arc::clone(&mailbox));
    self.reserve_name(parent.as_ref(), label, &self_ref, &strategy, strategy_inherited);
    // A child that a restart's wait brings in is kept by that restart, as
    // a child that was already in the table is: the restart's second half
    // restarts it with the others.
    if let Some(parent_ref) = parent.as_ref() {
      if let Some(entry) = self.parent_entry_mut(parent_ref) {
        if matches!(entry.waiting, Some(WaitReason::Recreation(_))) {
          if let Some(survivors) = entry.pending_survivors.as_mut() {
            survivors.push(self_ref.clone());
          }
        }
      }
    }
    let cell = ActorCell::pending(self_ref.clone(), parent.clone(), props, strategy);
    mailbox.put_back_cell(Box::new(cell));
    let entry = ActorEntry {
      self_ref: self_ref.clone(),
      parent: parent.clone(),
      mailbox: Arc::clone(&mailbox),
      stash: Vec::new(),
      capacities,
      supervision: None,
      deferred_replies: Vec::new(),
      perpetrator: None,
      system_stash: VecDeque::new(),
      restart_history: RestartHistory::new(),
    };
    self.actors.insert(self_ref.key(), entry);
    self.live.insert(self_ref.clone());
    // Pekko's `Dispatch.init` puts `Create` first on the system queue, and
    // `start` attaches the mailbox: `pre_start` runs in the child's own run
    // (section 11.12.2).
    Mailbox::system_send(&mailbox, SystemMessage::Create).ok();
    // A child started under a suspended parent takes the parent's suspend
    // on itself, one `Suspend` for each suspend the parent still waits
    // for an answer to, behind its `Create` (Pekko's `Children.makeChild`).
    // A parent without a mailbox (a guardian, a function reference) has no
    // suspend to pass on.
    if let Some(parent_ref) = parent.as_ref() {
      if let Some(parent_entry) = self.entry(parent_ref) {
        let inherited = parent_entry.mailbox.suspend_count();
        for _ in 0..inherited {
          Mailbox::system_send(&mailbox, SystemMessage::Suspend).ok();
        }
      }
    }
    self_ref
  }

  /// Takes `label` for `child` in the table of `parent` (Pekko's
  /// `reserveChild` and `initChild`), or among the top-level names when it has
  /// no parent. A parent that keeps no table yet gets one.
  fn reserve_name(
    &mut self,
    parent: Option<&ActorRef>,
    label: String,
    child: &ActorRef,
    strategy: &SupervisorStrategy,
    strategy_inherited: bool,
  ) {
    let Some(parent) = parent else {
      self.user_children.insert(label, child.clone());
      return;
    };
    if self.parent_entry(parent).is_none() {
      self.insert_parent_entry(ParentEntry::new(parent.clone(), strategy.clone()));
    }
    if let Some(entry) = self.parent_entry_mut(parent) {
      let spawn_strategy = if strategy_inherited { None } else { Some(strategy.clone()) };
      entry.children.push(ChildRecord {
        name: label,
        child: child.clone(),
        history: RestartHistory::new(),
        spawn_strategy,
      });
    }
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
      | None => self.insert_parent_entry(ParentEntry::new(parent, strategy)),
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
      self.insert_parent_entry(ParentEntry::new(parent.clone(), strategy.clone()));
    }
    self.spawn_named_child_under(parent, props, name, strategy, false)
  }

  /// Starts a named child once the checks shared by every named entry pass.
  ///
  /// The name is taken when the child is created and stays taken until the
  /// parent has handled the child's `ChildTerminated` (Pekko's
  /// `reserveChild`, and `removeChildAndGetStateChange`).
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
    if self.is_terminated() || self.is_terminating(parent) {
      self.hook_child_creation_refused = true;
      return Err(InvalidActorNameException::terminating());
    }
    validate_actor_name(name)?;
    if entry.children.iter().any(|record| record.name == name) {
      return Err(InvalidActorNameException::name_taken(name));
    }
    Ok(self.spawn_child_with_origin(parent, props, Some(name), strategy, strategy_inherited))
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
    if self.is_terminated() {
      return Err(InvalidActorNameException::terminating().into());
    }
    match self.parent_entry(parent) {
      | Some(entry) => {
        if entry.is_terminating() {
          return Err(InvalidActorNameException::terminating().into());
        }
        if let Some(name) = name {
          if entry.children.iter().any(|record| record.name == name) {
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
    // stays taken for both entries until the child stops, and its end
    // frees it for both.
    if let Some(name) = name {
      if parent == &self.user_guardian {
        self.user_children.insert(name.to_string(), child.clone());
      }
    }
    Ok(child)
  }

  /// Starts an actor a test wraps, attaching `handle` to its cell.
  ///
  /// The checks are the ones a top-level creation and
  /// [`try_spawn_child_from_weak`](Self::try_spawn_child_from_weak) run,
  /// in the same order: the name, that the system and the parent still
  /// take children, that the name is free, then the configuration. The
  /// strategy the child is placed under follows the kind of the
  /// supervisor: the user guardian's child takes the default strategy,
  /// a started actor's child inherits the strategy the parent applies
  /// now, and a function reference's child inherits the strategy
  /// registered for it, or takes the default when none is registered.
  ///
  /// # Panics
  ///
  /// Panics when the supervisor is an actor that has not started yet,
  /// with the cause Pekko's `TestActorRef` throws for that case.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name, the parent's state,
  /// or the configuration is refused.
  #[cfg(feature = "test-support")]
  pub(crate) fn try_spawn_test_actor(
    &mut self,
    supervisor: Option<&ActorRef>,
    props: Props,
    name: Option<&str>,
    handle: ActorHandle,
  ) -> Result<ActorRef, ActorCreationError> {
    if let Some(name) = name {
      validate_actor_name(name)?;
    }
    if self.is_terminated() {
      return Err(InvalidActorNameException::terminating().into());
    }
    let parent = supervisor.cloned().unwrap_or_else(|| self.user_guardian.clone());
    // The dispatcher kind the test reads is the kind the registry
    // resolved for the name the recipe carries (or the default name),
    // taken before the recipe moves into the cell.
    let dispatcher_name = props.dispatcher().unwrap_or(DEFAULT_DISPATCHER_ID).to_string();
    let dispatcher_kind = self.dispatchers.kind_of(&dispatcher_name);
    if parent == self.user_guardian {
      let label = match name {
        | Some(name) => name.to_string(),
        | None => alloc::format!("${}", next_spawn_name()),
      };
      let path = self.root.with_child("user").with_child(&label);
      if name.is_some() && self.user_children.contains_key(&label) {
        return Err(InvalidActorNameException::name_taken(&label).into());
      }
      self.verify_configuration(&props, &path)?;
      handle.set_dispatcher_kind(dispatcher_kind);
      handle.set_dead_letters(self.dead_letters());
      let child = self.spawn_at(props, path, None, SupervisorStrategy::default_strategy(), false);
      self.attach_test_handle(&child, handle);
      return Ok(child);
    }
    if let Some(entry) = self.parent_entry(&parent) {
      if entry.is_terminating() {
        return Err(InvalidActorNameException::terminating().into());
      }
      if let Some(name) = name {
        if entry.children.iter().any(|record| record.name == name) {
          return Err(InvalidActorNameException::name_taken(name).into());
        }
      }
    } else if !self.is_live(&parent) && !self.is_guardian(&parent) {
      return Err(InvalidActorNameException::terminating().into());
    }
    if self.is_terminating(&parent) {
      return Err(InvalidActorNameException::terminating().into());
    }
    let is_function_ref = self.function_refs.contains(&parent);
    let (strategy, inherited) = if is_function_ref {
      match self.parent_entry(&parent) {
        | Some(entry) => (entry.strategy.clone(), true),
        | None => (SupervisorStrategy::default_strategy(), false),
      }
    } else {
      // An actor supervisor must have started: a cell that is still in
      // its mailbox without an instance, and that is not running a
      // frame of its own start, has not.
      if let Some(created) = self.with_cell(&parent, ActorCell::is_created) {
        if !created && !self.is_running(&parent) {
          panic!(
            "cannot attach a TestActor to an unstarted top-level actor, ensure that it is started by sending a message and observing the reply"
          );
        }
      }
      let strategy = match self.parent_entry(&parent) {
        | Some(entry) => entry.strategy.clone(),
        | None => {
          self.with_cell(&parent, |cell| cell.strategy().clone()).unwrap_or_else(SupervisorStrategy::default_strategy)
        },
      };
      (strategy, true)
    };
    let path = match name {
      | Some(name) => parent.path().with_child(name),
      | None => parent.path().with_child("$generated"),
    };
    self.verify_configuration(&props, &path)?;
    handle.set_dispatcher_kind(dispatcher_kind);
    handle.set_dead_letters(self.dead_letters());
    let label = match name {
      | Some(name) => name.to_string(),
      | None => alloc::format!("${}", next_spawn_name()),
    };
    let path = parent.path().with_child(&label);
    let child = self.spawn_at(props, path, Some(parent.clone()), strategy, inherited);
    self.attach_test_handle(&child, handle);
    Ok(child)
  }

  /// Attaches `handle` to the cell of `child`, which `spawn_at` has
  /// just put into its mailbox.
  #[cfg(feature = "test-support")]
  fn attach_test_handle(&self, child: &ActorRef, handle: ActorHandle) {
    self.with_cell_mut(child, |cell| cell.set_handle(handle));
  }

  /// Runs `target`'s behaviour once with `message`, outside its
  /// mailbox.
  ///
  /// Mirrors `TestActorRef.receive`: the behaviour runs with `sender`
  /// in its context, the mailbox's queue is not touched, and a failure
  /// is returned as [`RunOutcome::Failed`] instead of reaching the
  /// supervisor, so the actor is not restarted for it. When the
  /// actor's dispatcher is of the calling-thread kind, the system
  /// messages this call queued for the actor itself (a stop it asked
  /// for) are handled before the call returns, as Pekko's
  /// calling-thread dispatcher handles a `systemDispatch` on the
  /// sender's thread.
  ///
  /// # Panics
  ///
  /// Panics when the actor has stopped, with the cause Pekko's
  /// `receive` raises for a cell whose actor is gone.
  #[cfg(feature = "test-support")]
  pub(crate) fn receive_directly(&mut self, target: &ActorRef, message: Message, sender: ActorRef) -> RunOutcome {
    if self.is_actor_terminated(target) {
      panic!("underlying actor is terminated");
    }
    let mailbox = self.entry(target).map(|entry| Arc::clone(&entry.mailbox));
    let mut cell = self.take_cell(target).expect("the actor that receives directly is in its mailbox");
    if !cell.is_created() {
      // An actor built a moment ago from inside another actor (the
      // nested creation `in_context` performs) may not have run its
      // `Create` yet: its cell holds no behaviour, and a receive now
      // would drop the message and report success. Start it first, as
      // a creation on the test thread has already started when a test
      // receives on it.
      if let Some(mailbox) = &mailbox {
        self.handle_create(&mut cell, mailbox);
      }
      if !cell.is_created() {
        if self.actors.contains_key(&target.key()) && self.entry(target).is_some_and(|entry| !entry.mailbox.has_cell())
        {
          self.put_cell(cell);
        }
        self.finish_run();
        return RunOutcome::Failed { message: "the actor failed to start".to_string() };
      }
    }
    let catcher = self.catcher.clone();
    self.enter_frame(target);
    let (outcome, _sender) = cell.receive(self, message, sender, catcher.as_deref());
    self.exit_frame();
    let kind = cell.handle().and_then(|handle| handle.dispatcher_kind());
    if kind == Some(DispatcherKind::CallingThread) {
      if let Some(mailbox) = &mailbox {
        mailbox.process_all_system_messages(&mut cell, self);
      }
    }
    if self.actors.contains_key(&target.key()) && self.entry(target).is_some_and(|entry| !entry.mailbox.has_cell()) {
      // The entry still stands and its mailbox slot is empty: the
      // actor lives on. A stop handled above removes the entry, and
      // then the cell is dropped with this call instead.
      self.put_cell(cell);
    }
    self.finish_run();
    outcome
  }

  /// Marks the start of one actor execution frame.
  fn enter_frame(&mut self, target: &ActorRef) {
    self.running.push(target.key());
  }

  /// Marks the end of one actor execution frame.
  fn exit_frame(&mut self) {
    self.running.pop();
  }

  /// Returns whether `target` runs a frame: its cell is out of its mailbox
  /// while it handles a message or runs a hook.
  fn is_running(&self, target: &ActorRef) -> bool {
    let key = target.key();
    self.running.contains(&key)
  }

  /// Handles a system message in the run of `cell` (Pekko's
  /// `ActorCell.systemInvoke`).
  ///
  /// A message the actor may not take yet is set aside in the actor's
  /// system stash: a suspended actor takes no `Failed`, and an actor that
  /// waits for its children takes no `Suspend`, `Resume`, `Recreate`, or
  /// `Failed`. `Create`, `Terminate`, and `ChildTerminated` are always
  /// taken. After each message the state is read again, and when it has
  /// gone down the messages set aside come back in the order they came,
  /// before the rest of the queue, and are judged again as they are
  /// taken, as Pekko's `invokeAll` prepends the unstashed messages and
  /// runs the list it built.
  fn system_invoke(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>, message: SystemMessage) {
    let self_ref = cell.self_ref();
    let mut queue: VecDeque<SystemMessage> = VecDeque::new();
    queue.push_back(message);
    while let Some(message) = queue.pop_front() {
      if self.should_stash(cell, mailbox, &message) {
        if let Some(entry) = self.entry_mut(&self_ref) {
          entry.system_stash.push_back(message);
        }
        continue;
      }
      let before = self.stash_state(cell, mailbox);
      self.handle_system(cell, mailbox, message);
      if self.entry(&self_ref).is_none() {
        // The actor has finished: what it had set aside dies with its
        // entry, as a closing mailbox discards its stash.
        return;
      }
      if self.stash_state(cell, mailbox) < before {
        if let Some(entry) = self.entry_mut(&self_ref) {
          let stashed: Vec<SystemMessage> = entry.system_stash.drain(..).collect();
          for message in stashed.into_iter().rev() {
            queue.push_front(message);
          }
        }
      }
    }
  }

  /// Returns how much of its system queue `cell` must set aside: nothing
  /// while it runs free, `Failed` only while it is suspended, and the
  /// wider set while it waits for its children (Pekko's
  /// `calculateState`). The suspend count is read from the mailbox, which
  /// is the one place that keeps it.
  fn stash_state(&self, cell: &ActorCell, mailbox: &Mailbox<ActorCell>) -> u8 {
    if self.is_waiting(&cell.self_ref()) {
      2
    } else if mailbox.suspend_count() > 0 {
      1
    } else {
      0
    }
  }

  /// Returns whether `message` is set aside in the state `cell` is in.
  fn should_stash(&self, cell: &ActorCell, mailbox: &Mailbox<ActorCell>, message: &SystemMessage) -> bool {
    match self.stash_state(cell, mailbox) {
      | 0 => false,
      | 1 => matches!(message, SystemMessage::Failed(_)),
      | _ => matches!(
        message,
        SystemMessage::Suspend
          | SystemMessage::Resume { .. }
          | SystemMessage::Recreate { .. }
          | SystemMessage::Failed(_)
      ),
    }
  }

  /// Handles one system message that the run of `cell` takes.
  fn handle_system(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>, message: SystemMessage) {
    match message {
      | SystemMessage::Create => self.handle_create(cell, mailbox),
      | SystemMessage::Suspend => self.fault_suspend(cell, mailbox),
      | SystemMessage::Resume { caused_by_failure } => self.fault_resume(cell, mailbox, caused_by_failure),
      | SystemMessage::Recreate { cause } => self.fault_recreate(cell, mailbox, cause),
      | SystemMessage::Failed(failure) => self.handle_failed(cell, mailbox, failure),
      | SystemMessage::Terminate => self.handle_terminate(cell, mailbox),
      | SystemMessage::ChildTerminated { child } => self.handle_child_terminated(cell, mailbox, &child),
    }
  }

  /// Handles `Create` (Pekko's `ActorCell.create`).
  ///
  /// A cell that already has its instance, that waits for children, or
  /// whose start failed does nothing: a failed start is tried again only
  /// by the `Recreate` the parent's decision sends, which reaches the
  /// same retry through `faultCreate`.
  fn handle_create(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>) {
    let self_ref = cell.self_ref();
    if cell.is_created() || cell.has_failed_start() || self.is_waiting(&self_ref) {
      return;
    }
    self.create_now(cell, mailbox);
  }

  /// Builds the instance of `cell` and runs its `pre_start` under the
  /// catcher (Pekko's `finishCreate`).
  ///
  /// A failure records the failed start and fails the actor in this run,
  /// as a failure of `receive` does. Pekko's `failActor` clears the
  /// behaviour stack before a retry, so a `become` issued by a `pre_start`
  /// that threw does not survive into the next creation.
  fn create_now(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>) {
    let self_ref = cell.self_ref();
    let catcher = self.catcher.clone();
    let mut stack = cell.take_stack();
    self.enter_frame(&self_ref);
    let outcome = {
      let mut context = ActorContext::new(&self_ref, self_ref.clone(), self, &mut stack);
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
        self.check_receive_timeout(self_ref.key(), true);
        cell.restore_stack(stack);
      },
      | RunOutcome::Failed { message } => {
        self.clear_receive_timeout(self_ref.key());
        cell.set_failed_sender(Some(self_ref.clone()));
        cell.record_start_failure();
        let failure = Failure {
          child: self_ref.clone(),
          kind: FailureKind::Initialization,
          cause_name: ACTOR_INITIALIZATION_CAUSE,
          message,
        };
        self.handle_invoke_failure(cell, mailbox, failure, None);
      },
    }
  }

  /// Asks every child of `parent` to stop and records that `parent` waits for
  /// them for `reason`. Returns whether there is a child to wait for. A
  /// parent that stops is marked stopping even without children, so its own
  /// `post_stop` cannot start one.
  fn stop_children_and_wait(&mut self, parent: &ActorRef, reason: WaitReason) -> bool {
    let children = self.parent_entry_mut(parent).map(|entry| entry.wait_for_all_children(reason)).unwrap_or_default();
    for child in &children {
      self.stop(child);
    }
    !children.is_empty()
  }

  /// Asks every child of `parent` to stop (`ActorContext.stop` on each).
  pub(crate) fn stop_children_of(&mut self, parent: &ActorRef) {
    for child in self.children_of(parent) {
      self.stop(&child);
    }
  }

  /// Asks the actor or function reference `target` to stop.
  ///
  /// Mirrors `ActorCell.stop` and `Children.stop`: an actor gets a
  /// `Terminate` on its system queue, which its own run handles (design of
  /// #234, section 11.12.3), and its parent counts it among the children that
  /// are to die. Stopping an actor that has stopped does nothing. A function
  /// reference has no mailbox, so the executor ends it here: it closes its
  /// boundary, which keeps its handler from running again, stops its
  /// children, and tells its watchers once the last child has gone.
  pub(crate) fn stop(&mut self, target: &ActorRef) {
    if self.function_refs.contains(target) {
      self.stop_function_ref(target);
      return;
    }
    let Some(entry) = self.entry(target) else {
      return;
    };
    let mailbox = Arc::clone(&entry.mailbox);
    if let Some(record) = entry.parent.clone().and_then(|parent| self.parent_entry_mut(&parent)) {
      record.mark_dying(target);
    }
    if let Err(message) = Mailbox::system_send(&mailbox, SystemMessage::Terminate) {
      self.route_closed_system_message(message, target.clone());
    }
  }

  /// Handles `Terminate` (Pekko's `FaultHandling.terminate`, section
  /// 11.12.3).
  ///
  /// An actor that already stops does nothing. Otherwise it stops watching
  /// every actor it watches (T1, Pekko's `unwatchWatchedActors`), asks every
  /// child to stop, and with children left waits for them suspended, so that
  /// it takes no normal message meanwhile; without, it finishes at once.
  fn handle_terminate(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>) {
    let self_ref = cell.self_ref();
    if self.is_terminating(&self_ref) {
      return;
    }
    self.clear_receive_timeout(self_ref.key());
    self.unwatch_all_of(&self_ref);
    if self.stop_children_and_wait(&self_ref, WaitReason::Termination) {
      mailbox.suspend();
      return;
    }
    self.finish_terminate(cell);
  }

  /// Finishes the stop of `cell` (Pekko's `finishTerminate`): `post_stop`
  /// and the function references (T2), the stash and the queue to dead
  /// letters as the mailbox closes (T3), the parent's `ChildTerminated` and
  /// then the watchers' `Terminated` (T4), the termination callbacks and the
  /// replies of `stop_and_reply` (T5).
  ///
  /// The parent learns first: a parent that also watches handles the
  /// `ChildTerminated` before the `Terminated`, as Pekko's notes on
  /// `finishTerminate` require. A parent without a mailbox (a guardian, a
  /// function reference) frees the name here. The cell itself is dropped by
  /// the run once it returns, outside the mailbox's lock (section 11.6, L3).
  fn finish_terminate(&mut self, cell: &mut ActorCell) {
    let target = cell.self_ref();
    self.stop_function_refs_of(&target);
    if !cell.post_stop_suppressed() {
      let sender = cell.failed_sender();
      self.run_post_stop(cell, sender);
    }
    // A post_stop hook receives a context and can create function
    // references; those belong to the stopping actor and end here too.
    self.stop_function_refs_of(&target);
    self.queued_to_dead_letters(&target);
    match cell.parent() {
      | Some(parent) => {
        let parent_mailbox = self.entry(&parent).map(|entry| Arc::clone(&entry.mailbox));
        match parent_mailbox {
          | Some(mailbox) => {
            let notice = SystemMessage::ChildTerminated { child: target.clone() };
            if let Err(message) = Mailbox::system_send(&mailbox, notice) {
              self.route_closed_system_message(message, parent.clone());
            }
          },
          | None => self.child_ended_under(&parent, &target),
        }
      },
      | None => {},
    }
    let child_path = target.path();
    if self.user_children.get(child_path.name()) == Some(&target) {
      self.user_children.remove(child_path.name());
    }
    #[cfg(feature = "test-support")]
    if let Some(handle) = cell.handle() {
      // The handle learns of the stop before any watcher is told, so
      // a test that reads the actor as it stops answers at once.
      handle.mark_terminated();
    }
    self.notify_watchers(&target, true, false);
    self.run_termination_callbacks(&target);
    self.flush_deferred_replies(&target);
    self.clear_receive_timeout(target.key());
    if let Some(entry) = self.actors.remove(&target.key()) {
      drop(entry);
    }
    self.ref_parents.retain(|entry| entry.parent != target);
  }

  /// Frees the name of `child` under `parent`, a parent without a mailbox,
  /// in the stop of the child itself (section 11.12.4). A function reference
  /// that waits for its children tells its watchers once the last has gone.
  fn child_ended_under(&mut self, parent: &ActorRef, child: &ActorRef) {
    let ended = self.parent_entry_mut(parent).and_then(|entry| entry.remove_child(child)).flatten();
    if matches!(ended, Some(WaitReason::Termination)) {
      self.notify_watchers(parent, true, false);
      self.run_termination_callbacks(parent);
      self.ref_parents.retain(|entry| &entry.parent != parent);
    }
  }

  /// Handles `ChildTerminated` (Pekko's `FaultHandling.handleChildTerminated`,
  /// section 11.12.4).
  ///
  /// The child leaves the table only when the handle in the notice is the
  /// one the table holds, so the notice of an old child never frees the name
  /// of a new one. The parent's child-terminated hook runs next, and the work
  /// the parent waited for goes on once the last dying child has gone.
  fn handle_child_terminated(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>, child: &ActorRef) {
    let self_ref = cell.self_ref();
    let Some(next) = self.parent_entry_mut(&self_ref).and_then(|entry| entry.remove_child(child)) else {
      return;
    };
    if let Some((cause_name, message)) = self.run_child_terminated_hook(cell, child) {
      // The hook's failure is reported here once, and then fails the
      // parent in this run with the failure the hook itself raised, so
      // the decision above the parent reports that failure, and not
      // this report, a second time.
      self.publish_child_failure(
        &self_ref,
        HANDLE_CHILD_TERMINATED_FAILED_CAUSE,
        HANDLE_CHILD_TERMINATED_FAILED_MESSAGE,
      );
      let failure = Failure { child: self_ref.clone(), kind: FailureKind::MessageHandling, cause_name, message };
      self.handle_invoke_failure(cell, mailbox, failure, None);
    }
    match next {
      | Some(WaitReason::Termination) => self.finish_terminate(cell),
      | Some(WaitReason::Creation) => self.finish_create(cell, mailbox),
      | Some(WaitReason::Recreation(cause)) => self.finish_recreate(cell, mailbox, cause),
      | None => {},
    }
  }

  /// Runs the child-terminated hook of the parent `cell` for `child`, and
  /// returns the failure the hook raised when it failed.
  ///
  /// Mirrors `FaultHandling.handleChildTerminated`: the parent's strategy hook
  /// runs with the parent's context, so it may start a replacement child. A
  /// hook that throws fails with the panic it raised; a hook that tries to
  /// create a child while the parent is terminating fails with the refusal
  /// Pekko's `attachChild` throws. The hook does not run for a parent that
  /// never finished starting, matching Pekko's `if (actor != null)` guard.
  fn run_child_terminated_hook(&mut self, cell: &mut ActorCell, child: &ActorRef) -> Option<(&'static str, String)> {
    let parent = cell.self_ref();
    let entry = self.parent_entry(&parent)?;
    if !entry.strategy.has_child_terminated() || !cell.is_created() {
      return None;
    }
    let strategy = entry.strategy.clone();
    let siblings = self.open_children_of(&parent);
    let mut stack = cell.take_stack();
    self.hook_child_creation_refused = false;
    self.enter_frame(&parent);
    let catcher = self.catcher.clone();
    let outcome = {
      let mut context = ActorContext::new(&parent, parent.clone(), self, &mut stack);
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
    let refused = core::mem::replace(&mut self.hook_child_creation_refused, false);
    match outcome {
      | RunOutcome::Failed { message } => Some((RUNTIME_CAUSE, message)),
      | RunOutcome::Completed if refused => {
        Some((HANDLE_CHILD_TERMINATED_FAILED_CAUSE, TERMINATED_MESSAGE.to_string()))
      },
      | RunOutcome::Completed => None,
    }
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

  /// Applies `owner`'s bounded mailbox capacity to a returning stash,
  /// counting the `queued_backlog` envelopes already in the mailbox
  /// against the capacity: the oldest excess envelopes are the overflow
  /// (returned in the order they go to dead letters, newest victim
  /// first), and the newest envelopes that fit are returned in stash
  /// order after them.
  ///
  /// The backlog is an argument because the stop-time clean-up reads it
  /// before the mailbox closes: closing drains the queue, and a backlog
  /// read afterwards would always be zero.
  fn split_stash_overflow(
    &self,
    owner: &ActorRef,
    stash: Vec<(u64, Envelope)>,
    queued_backlog: usize,
  ) -> (Vec<Envelope>, Vec<Envelope>) {
    let (mailbox_capacity, _) = self.capacities_of(owner);
    let room = mailbox_capacity.bound().map(|bound| bound.saturating_sub(queued_backlog)).unwrap_or(usize::MAX);
    let total = stash.len();
    let mut envelopes = stash.into_iter().map(|(_, envelope)| envelope);
    if total > room {
      let victims: Vec<_> = envelopes.by_ref().take(total - room).collect();
      let victims: Vec<_> = victims.into_iter().rev().collect();
      return (victims, envelopes.collect());
    }
    (Vec::new(), envelopes.collect())
  }

  /// Sends a returning stash's overflow to dead letters and returns the
  /// envelopes that fit, as [`split_stash_overflow`](Self::split_stash_overflow)
  /// divides them against the backlog now in `owner`'s mailbox.
  fn discard_stash_overflow(&mut self, owner: &ActorRef, stash: Vec<(u64, Envelope)>) -> Vec<Envelope> {
    let backlog = self.entry(owner).map_or(0, |entry| entry.mailbox.number_of_messages());
    let (victims, remaining) = self.split_stash_overflow(owner, stash, backlog);
    self.publish_envelopes_to_dead_letters(victims, owner);
    remaining
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
    // The backlog is read before the mailbox closes, and the stash is
    // taken and divided against it here: closing drains the queue, so a
    // backlog read afterwards would give the stash overflow the whole
    // capacity as its room. Dividing the stash here also fixes the order
    // the clean-up publishes in, below.
    let taken = self.entry_mut(target).map(|entry| {
      let backlog = entry.mailbox.number_of_messages();
      (backlog, core::mem::take(&mut entry.stash))
    });
    let Some((backlog, stash)) = taken else {
      return;
    };
    let (overflow, remaining) = self.split_stash_overflow(target, stash, backlog);
    let Some(entry) = self.entry(target) else {
      return;
    };
    let mailbox = Arc::clone(&entry.mailbox);
    // The order mirrors Pekko's clean-up: the stash overflow goes first,
    // before the mailbox closes (in `postStop`'s `unstashAll` the
    // overflow is refused and routed before the mailbox clean-up runs),
    // then the system messages the closed mailbox still held (a
    // `Terminate` is suppressed, a `Resume` or a `Create` is a dead
    // letter), then the rest of the stash, and then the queued
    // envelopes. A stash that `postStop` already returned through
    // `unstash_all` is empty here and rides in the envelopes instead,
    // in the same place.
    self.publish_envelopes_to_dead_letters(overflow, target);
    self.live.remove(target);
    let (envelopes, system) = mailbox.close();
    let runs = self.manual.take_runs_of(&mailbox);
    drop(runs);
    for message in system {
      self.route_closed_system_message(message, target.clone());
    }
    self.publish_envelopes_to_dead_letters(remaining, target);
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

  /// Registers a reply to `reply_to` after `target` stops, and asks
  /// `target` to stop. The reply goes out once the stop has finished (T5).
  pub(crate) fn stop_and_reply(&mut self, target: &ActorRef, reply_to: ActorRef, message: Message) {
    let entry = self.entry_mut(target).expect("only an actor with an entry stops after a reply");
    entry.deferred_replies.push(DeferredReply { reply_to, message });
    self.stop(target);
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

  /// Clears every watch `target` holds as a watcher (Pekko's
  /// `unwatchWatchedActors`), so no `Terminated` reaches it while it stops.
  fn unwatch_all_of(&mut self, target: &ActorRef) {
    self.function_watching.retain(|(watcher, _)| watcher != target);
    self.watchers.retain(|(watcher, _)| watcher != target);
    self.watch_messages.retain(|(watcher, _, _)| watcher != target);
    self.pending_terminations.retain(|pending| &pending.watcher != target);
  }

  /// Runs the registered termination callbacks for `target`.
  fn run_termination_callbacks(&self, target: &ActorRef) {
    for callback in &self.termination_callbacks {
      callback(target);
    }
  }

  /// Ends one live function reference: closes its boundary and clears the
  /// watches it held. Without children it tells its watchers at once; with
  /// children it asks them to stop, and the last one's end tells the watchers
  /// (T1 before T4, section 11.12.3).
  fn stop_function_ref(&mut self, target: &ActorRef) {
    self.close_function_ref(target);
    self.stop_function_refs_of(target);
    self.unwatch_all_of(target);
    if self.stop_children_and_wait(target, WaitReason::Termination) {
      return;
    }
    self.notify_watchers(target, true, false);
    self.run_termination_callbacks(target);
    self.ref_parents.retain(|entry| &entry.parent != target);
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
  /// throughput at a time.
  pub(crate) fn run_until_idle(&mut self) {
    loop {
      self.fire_due_timers();
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
    self.fire_due_timers();
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

  /// Does what a run left, now that the actor is back in its mailbox:
  /// the timers that came due while it ran fire. A failure the run met
  /// was handled inside the run itself, so nothing waits for this.
  fn finish_run(&mut self) {
    self.fire_due_timers();
  }

  /// Hands one normal message to the actor of `cell` (Pekko's
  /// `ActorCell.invoke`), and says whether its run may take the next.
  ///
  /// A failure, and a `Kill`, are handled inside this run, as Pekko's
  /// `handleInvokeFailure` handles them: the mailbox is suspended, the
  /// children are suspended, and the parent is told, so the run goes on
  /// with the next message only as the mailbox allows. A `PoisonPill`
  /// puts a `Terminate` on the actor's own system queue (Pekko's
  /// `autoReceiveMessage`), which the run handles before the next message.
  fn invoke(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>, envelope: Envelope) -> Next {
    let (message, sender) = envelope.into_message();
    let message = match message.downcast_ref::<Arc<PendingTermination>>() {
      | Some(pending) => match self.take_pending_termination(pending) {
        | Some(message) => message,
        | None => {
          let key = cell.key();
          let timeout_before = self.receive_timeout_before(key, true);
          self.receive_timeout_after(key, true, timeout_before);
          self.fire_due_timers();
          return Next::Continue;
        },
      },
      | None => message,
    };
    cell.set_failed_sender(None);
    #[cfg(feature = "test-support")]
    if let Some(control) = message.downcast_ref::<ActorControl>() {
      // A test's probe is answered here, before the behaviour can see
      // it, as Pekko's cell answers `InternalGetActor` in
      // `autoReceiveMessage`: a behaviour installed with
      // `change_behavior` and a message held in the stash cannot hide
      // the actor from the test. The probe is an ordinary message for
      // the receive timeout, so the deadline is re-armed around it as
      // around any other delivery.
      let sequence = control.sequence;
      let key = cell.key();
      let influences = !self.is_not_influence_receive_timeout(&message);
      let timeout_before = self.receive_timeout_before(key, influences);
      if let Some(handle) = cell.handle().cloned() {
        // A snapshot hook that panics fails only this probe: the
        // catcher turns it into no snapshot, which the test reads as
        // the failure, instead of the panic unwinding out of the run
        // and being recorded as the executor's fatal failure, which
        // would stop every actor in the system.
        let snapshot = match self.catcher.clone() {
          | Some(catcher) => {
            let mut taken = None;
            match catcher.run(&mut || taken = cell.probe_snapshot()) {
              | RunOutcome::Completed => taken,
              | RunOutcome::Failed { .. } => None,
            }
          },
          | None => cell.probe_snapshot(),
        };
        let setting = self.receive_timeout_setting(key);
        handle.note_probe(sequence, snapshot, setting);
      }
      self.receive_timeout_after(key, influences, timeout_before);
      self.fire_due_timers();
      return Next::Continue;
    }
    if message == KILL {
      let key = cell.key();
      let timeout_before = self.receive_timeout_before(key, true);
      cell.set_failed_sender(Some(sender.clone()));
      let failure = Failure {
        child: cell.self_ref(),
        kind: FailureKind::Killed,
        cause_name: ACTOR_KILLED_CAUSE,
        message: ACTOR_KILLED_MESSAGE.to_string(),
      };
      self.handle_invoke_failure(cell, mailbox, failure, None);
      self.receive_timeout_after(key, true, timeout_before);
      self.fire_due_timers();
      return Next::Continue;
    }
    if message == POISON_PILL {
      let key = cell.key();
      let timeout_before = self.receive_timeout_before(key, true);
      mailbox.system_enqueue(SystemMessage::Terminate).ok();
      self.receive_timeout_after(key, true, timeout_before);
      self.fire_due_timers();
      return Next::Continue;
    }
    let key = cell.key();
    let influences = !self.is_not_influence_receive_timeout(&message);
    let timeout_before = self.receive_timeout_before(key, influences);
    let catcher = self.catcher.clone();
    self.running.push(key);
    let (outcome, sender) = cell.receive(self, message, sender, catcher.as_deref());
    self.exit_frame();
    self.receive_timeout_after(key, influences, timeout_before);
    self.fire_due_timers();
    match outcome {
      | RunOutcome::Completed => Next::Continue,
      | RunOutcome::Failed { message } => {
        cell.set_failed_sender(Some(sender.clone()));
        // A stop after a reply, which the message asked for before it
        // failed, wins: its `Terminate` is queued, and no decision applies.
        if self.actors.get(&key).is_some_and(|entry| !entry.deferred_replies.is_empty()) {
          return Next::Continue;
        }
        let failure =
          Failure { child: cell.self_ref(), kind: FailureKind::MessageHandling, cause_name: RUNTIME_CAUSE, message };
        self.handle_invoke_failure(cell, mailbox, failure, None);
        Next::Continue
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
    self.fire_due_timers();
    let arrived = self.arrivals.as_ref().map_or(0, |arrivals| arrivals.count());
    let has_work = |executor: &Self| {
      !executor.manual.is_empty()
        || executor.boundary.has_pending()
        || executor.arrivals.as_ref().is_some_and(|arrivals| arrivals.count() != arrived)
    };
    if has_work(self) {
      return true;
    }
    // The wait is cut at the earliest receive timeout deadline: a wait
    // that the deadline cuts short fires the timeout, puts its message
    // in the mailbox, and returns `true`, as a wait that work ends does.
    let wait_cap = match self.earliest_timer_deadline() {
      | Some(deadline) => match self.now_opt() {
        | Some(now) => max.min(deadline.saturating_sub(now)),
        | None => max,
      },
      | None => max,
    };
    let Some(arrivals) = self.arrivals.clone() else {
      // Without a signal, a send to a mailbox from another thread wakes
      // nothing: the boundary is polled in short slices, and the mailboxes
      // are looked at between them.
      let mut waited = Duration::ZERO;
      while waited < wait_cap {
        let slice = (wait_cap - waited).min(POLL_SLICE);
        if let Some(envelope) = self.boundary.poll_within(slice) {
          self.manual.submit(ManualTask::Envelope(envelope));
          return true;
        }
        waited += slice;
        self.fire_due_timers();
        if has_work(self) {
          return true;
        }
      }
      self.fire_due_timers();
      return has_work(self);
    };
    // With no receive timeout scheduled, the bound of the wait is
    // measured on the boundary clock, not on a clock installed for the
    // actors: a hand-driven installed clock does not advance while this
    // thread waits in real time, so it would never reach the bound when
    // stale signals keep ending the inner waits.
    let use_boundary_clock = self.earliest_timer_deadline().is_none() && self.boundary.monotonic_time().is_some();
    let wait_clock = |executor: &Self| {
      if use_boundary_clock {
        executor.boundary.monotonic_time()
      } else {
        executor.now_opt()
      }
    };
    let Some(start) = wait_clock(self) else {
      // Without a clock, a wait that a stale signal ends is started again
      // for the capped wait; only a wait that sees no signal at all ends
      // with `false`.
      while arrivals.wait(wait_cap) {
        if has_work(self) {
          return true;
        }
      }
      return has_work(self);
    };
    // The cap was measured before `start` was read; time that passed
    // between the two readings would otherwise delay the firing by the
    // same amount. Measure the cap again from `start`: a deadline that
    // has already come fires at once, without waiting.
    let wait_cap = match self.earliest_timer_deadline() {
      | Some(deadline) if deadline <= start => {
        self.fire_due_timers();
        return has_work(self);
      },
      | Some(deadline) => max.min(deadline - start),
      | None => max,
    };
    let deadline = start.saturating_add(wait_cap);
    let mut unchanged_waits = 0_u32;
    loop {
      let now = wait_clock(self).unwrap_or(deadline);
      if now >= deadline {
        self.fire_due_timers();
        return has_work(self);
      }
      let signaled = arrivals.wait(deadline - now);
      self.fire_due_timers();
      if has_work(self) {
        return true;
      }
      if signaled {
        // The clock that bounds this wait is expected to advance with
        // real time while the thread waits (the boundary clock's
        // contract when it reports one). A clock that stands still
        // through many signaled waits does not: counting those waits
        // ends the wait instead of turning on the same reading forever,
        // as a wait whose clock reaches the deadline ends.
        if wait_clock(self).is_some_and(|reading| reading > now) {
          unchanged_waits = 0;
        } else {
          unchanged_waits += 1;
          if unchanged_waits >= 128 {
            return has_work(self);
          }
        }
      }
      if !signaled {
        // The real wait ran out without a signal. A clock the caller
        // drives by hand does not advance while this thread waits, so
        // waiting again on the same reading would never end: the wait
        // is over, as it is when the clock itself reaches the deadline.
        return has_work(self);
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
    self.in_run = false;
    self.watchers.clear();
    self.watch_messages.clear();
    self.pending_terminations.clear();
    self.user_children.clear();
    self.receive_timeouts.clear();
    self.receive_timeout_index.clear();
    self.receive_timeout_indexed.clear();
    self.fsm_timers.clear();
    self.fsm_timer_index.clear();
    self.running.clear();
    self.termination_callbacks.clear();
    while self.boundary.poll().is_some() {}
  }

  /// Records that a caller's thread unwound out of the executor: a panic that
  /// no catcher caught, which Pekko treats as fatal. The frames that were running are forgotten
  /// without running user code, and the next call into the executor terminates the system.
  fn mark_fatal(&mut self) {
    self.fatal = true;
    self.running.clear();
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
  /// that. A refusal the executor raises
  /// before it changes anything, as Pekko throws to the caller, leaves the
  /// system as it was.
  pub(crate) fn lend<R>(executor: &spin::Mutex<Executor>, work: impl FnOnce(&mut Executor) -> R) -> R {
    let mut guard = lock(executor);
    if guard.fatal {
      guard.stop_all();
    }
    let mut lent = Lent { executor: &mut guard, returned: false };
    #[cfg(feature = "test-support")]
    let _registration = crate::adaptor::LentRegistration::register(executor, lent.executor);
    let result = work(lent.executor);
    lent.returned = true;
    result
  }

  /// Runs `work` on the executor behind `executor`, as [`lend`](Self::lend)
  /// does, except that a call made from inside a lend of the same executor
  /// on the same thread reuses that lend instead of locking again.
  ///
  /// A test's entry reached from inside an actor — a `TestActorRef`
  /// built from `context.system()` while the actor runs — arrives while
  /// the executor is already lent to the run that is handling the
  /// actor, and the lock would refuse the re-entry as a concurrent use.
  /// The reuse is sound because the registered pointer names the
  /// executor of the lend this call stack is inside: it stays alive,
  /// unmoved, and exclusively borrowed by this thread until that lend
  /// returns, and the registration is removed when it does.
  #[cfg(feature = "test-support")]
  pub(crate) fn lend_or_reuse<R>(executor: &spin::Mutex<Executor>, work: impl FnOnce(&mut Executor) -> R) -> R {
    if let Some(pointer) = crate::adaptor::active_lent_executor(executor) {
      // SAFETY: the pointer was registered by the lend of this same
      // mutex that is active on this thread, so the executor it
      // names is exclusively borrowed by this call stack and
      // outlives the call.
      return work(unsafe { &mut *pointer });
    }
    Self::lend(executor, work)
  }

  /// Returns whether the system has been terminated.
  pub(crate) fn is_terminated(&self) -> bool {
    self.routes_closed.load(Ordering::SeqCst)
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
    assert!(!self.is_terminated(), "{TERMINATED_MESSAGE}");
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
  /// `post_restart`, or a child-terminated hook. A function reference that
  /// has been stopped but waits for its children counts as live too: its
  /// watchers are told once the last child has gone.
  fn is_live(&self, target: &ActorRef) -> bool {
    self.has_cell(target)
      || self.function_refs.contains(target)
      || self.is_running(target)
      || self.ref_parents.iter().any(|entry| &entry.parent == target && entry.is_terminating())
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
      // executor can be borrowed.
      if let Some(listener) = self.dead_letter_listener.take() {
        self.stop(&listener);
      }
      return;
    }
    if let Some(selection) = message.downcast_ref::<ActorSelectionMessage>() {
      let selection = selection.clone();
      self.deliver_selection(selection.path(), selection.message().clone(), selection.sender().clone());
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
          mailbox.route_refused(refused.with_recipient(recipient));
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
      return;
    }
    self.route_undeliverable(message, sender, recipient);
  }

  /// Handles a failure of `cell` inside its own run (Pekko's
  /// `handleInvokeFailure`).
  ///
  /// The mailbox is suspended once, the perpetrator of the failure is
  /// recorded while no failure waits for an answer yet, every child but
  /// the perpetrator is suspended, and the parent is told with a
  /// `Failed`. A parent without a mailbox (a guardian, a function
  /// reference) decides on the spot instead, as it does for an actor
  /// that has no parent.
  ///
  /// `perpetrator` names the child whose `Failed` the decider panicked
  /// on, when the failure is that panic; the caller leaves it empty for
  /// the actor's own failures, and the actor itself is recorded.
  fn handle_invoke_failure(
    &mut self,
    cell: &mut ActorCell,
    mailbox: &Mailbox<ActorCell>,
    failure: Failure,
    perpetrator: Option<ActorRef>,
  ) {
    let self_ref = cell.self_ref();
    mailbox.suspend();
    let recorded = self.entry(&self_ref).and_then(|entry| entry.perpetrator.clone());
    let perpetrator = match recorded {
      | Some(existing) => existing,
      | None => {
        let perpetrator = perpetrator.unwrap_or_else(|| self_ref.clone());
        if let Some(entry) = self.entry_mut(&self_ref) {
          entry.perpetrator = Some(perpetrator.clone());
        }
        perpetrator
      },
    };
    for child in self.children_of(&self_ref) {
      if child != perpetrator {
        self.send_system(&child, SystemMessage::Suspend);
      }
    }
    match cell.parent() {
      | None => self.inline_decide(cell, failure, None),
      | Some(parent) => {
        if self.entry(&parent).is_some() {
          self.send_system(&parent, SystemMessage::Failed(failure));
        } else {
          self.inline_decide(cell, failure, Some(parent));
        }
      },
    }
  }

  /// Handles the `Failed` of a child in the run of `cell`, its parent
  /// (Pekko's `handleFailure`).
  ///
  /// A failure of a child that is no longer in the table, under the same
  /// identifier and generation, is dropped. The failure is published,
  /// the strategy is chosen and its decider run under the catcher, and
  /// the directive is applied: a restart sends the child a `Recreate`, a
  /// stop sends it a `Terminate`, and a decider panic fails the parent
  /// in this run, with the child as the perpetrator.
  fn handle_failed(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>, failure: Failure) {
    let self_ref = cell.self_ref();
    if !self.is_child_in_table(&self_ref, &failure.child) {
      return;
    }
    self.publish_child_failure(&failure.child, failure.cause_name, &failure.message);
    let inputs = self.decision_inputs(&self_ref, &failure, cell.parent().is_some());
    match self.choose_decision(&failure.child, failure.kind, &inputs) {
      | Decision::Apply(directive, strategy, governing_all_for_one) => {
        let cause = RestartCause::of(&failure);
        self.apply_directive(Some(&self_ref), &failure, directive, &strategy, governing_all_for_one, cause);
      },
      | Decision::DeciderPanicked { message } => {
        let perpetrator = failure.child.clone();
        let own_failure =
          Failure { child: self_ref.clone(), kind: FailureKind::MessageHandling, cause_name: RUNTIME_CAUSE, message };
        self.handle_invoke_failure(cell, mailbox, own_failure, Some(perpetrator));
      },
    }
  }

  /// Decides for `failure` on the spot, for a parent that has no mailbox
  /// of its own to run the decision in (a guardian, a function
  /// reference).
  ///
  /// The failure is published, and the directive is applied as a
  /// parent's run applies it. A decider panic here has no parent run to
  /// fail in: the panic is published once more, and a child with an
  /// instance is resumed with the cause of its failure, while a child
  /// whose start failed is left as it is.
  fn inline_decide(&mut self, cell: &mut ActorCell, failure: Failure, parent: Option<ActorRef>) {
    self.publish_child_failure(&failure.child, failure.cause_name, &failure.message);
    let inputs = match &parent {
      | Some(parent) => self.decision_inputs(parent, &failure, self.function_refs.contains(parent)),
      | None => DecisionInputs {
        governing: None,
        spawn_strategy: Some(cell.strategy().clone()),
        count: self.entry(&failure.child).map_or(0, |entry| entry.restart_history.count()),
        escalation: self.is_escalation(&failure.child),
        parent_not_top: false,
      },
    };
    match self.choose_decision(&failure.child, failure.kind, &inputs) {
      | Decision::Apply(directive, strategy, governing_all_for_one) => {
        let cause = RestartCause::of(&failure);
        self.apply_directive(parent.as_ref(), &failure, directive, &strategy, governing_all_for_one, cause);
      },
      | Decision::DeciderPanicked { message, .. } => {
        self.publish_child_failure(&failure.child, RUNTIME_CAUSE, &message);
        if cell.is_created() {
          self.send_system(&failure.child, SystemMessage::Resume { caused_by_failure: true });
        }
      },
    }
  }

  /// Returns whether the recorded failure of `child` answers for one of
  /// its own children: its decider panicked on a child's `Failed`, so
  /// the failure the parent above decides for is an escalation.
  fn is_escalation(&self, child: &ActorRef) -> bool {
    self.entry(child).and_then(|entry| entry.perpetrator.clone()).is_some_and(|perpetrator| &perpetrator != child)
  }

  /// Reads what the decision for `failure` of a child in the table of
  /// `table_parent` is made from: the strategy the table's owner applies
  /// to its children, the strategy the child was started under when it
  /// did not inherit, and the restarts the child has used.
  ///
  /// `parent_not_top` tells whether the deciding parent itself stands
  /// under a parent (an actor parent with a parent of its own, or a
  /// function reference), which is what lets a strategy the child was
  /// started under decide for it.
  fn decision_inputs(&self, table_parent: &ActorRef, failure: &Failure, parent_not_top: bool) -> DecisionInputs {
    let governing = self.parent_entry(table_parent).map(|entry| entry.strategy.clone());
    let (count, spawn_strategy) =
      match self.parent_entry(table_parent).and_then(|entry| entry.child_record(&failure.child)) {
        | Some(record) => (record.history.count(), record.spawn_strategy.clone()),
        | None => (0, None),
      };
    DecisionInputs { governing, spawn_strategy, count, escalation: self.is_escalation(&failure.child), parent_not_top }
  }

  /// Chooses the directive for a failure from `inputs`, running a
  /// decider under the catcher when one is chosen.
  ///
  /// The decider is the first of these that a caller installed: the
  /// decider of the strategy the deciding parent applies to its
  /// children; and the decider of the strategy the child was started
  /// under, when the deciding parent stands under a parent itself, when
  /// the failure is an escalation, or when the failure is of the child's
  /// start, which the strategy it was started under answers for. With no
  /// decider chosen, the strategy the
  /// child was started under, or the deciding parent's when it
  /// inherited, decides by its policy alone. An all-for-one strategy,
  /// the deciding parent's or the one the child was started under,
  /// decides for the whole set of siblings at once.
  fn choose_decision(&self, child: &ActorRef, kind: FailureKind, inputs: &DecisionInputs) -> Decision {
    let all_for_one = inputs
      .governing
      .clone()
      .filter(|strategy| strategy.is_all_for_one())
      .or_else(|| inputs.spawn_strategy.clone().filter(|strategy| strategy.is_all_for_one()));
    if let Some(strategy) = all_for_one {
      return match self.decide_catching(strategy.clone(), child, inputs.count, kind) {
        | Ok(directive) => Decision::Apply(directive, strategy, true),
        | Err(message) => Decision::DeciderPanicked { message },
      };
    }
    let mut decider: Option<SupervisorStrategy> = None;
    if let Some(governing) = &inputs.governing {
      if governing.has_custom_decider() {
        decider = Some(governing.clone());
      }
    }
    if decider.is_none() {
      if let Some(spawn) = &inputs.spawn_strategy {
        let speaks = inputs.parent_not_top || inputs.escalation || kind == FailureKind::Initialization;
        if spawn.has_custom_decider() && speaks {
          decider = Some(spawn.clone());
        }
      }
    }
    if let Some(strategy) = decider {
      return match self.decide_catching(strategy.clone(), child, inputs.count, kind) {
        | Ok(directive) => Decision::Apply(directive, strategy, false),
        | Err(message) => Decision::DeciderPanicked { message },
      };
    }
    let strategy = inputs
      .spawn_strategy
      .clone()
      .or_else(|| inputs.governing.clone())
      .unwrap_or_else(SupervisorStrategy::default_strategy);
    Decision::Apply(strategy.decide(inputs.count, kind), strategy, false)
  }

  /// Applies a chosen directive for `failure`.
  ///
  /// A restart the strategy still has to permit is asked against the
  /// child's restart record: a granted restart sends the child a
  /// `Recreate` with the failure's cause, a refused one stops it. A stop
  /// sends a `Terminate`. Under an all-for-one strategy a restart is
  /// granted only when every child in the table may still restart,
  /// asked in the order the children stand in the table and stopping at
  /// the first refusal, whose own request is still counted: the granted
  /// restart suspends every sibling but the failed child and recreates
  /// them all, and a refused one, like a stop, stops them all.
  ///
  /// The record a permission is asked against is the child's record in
  /// the table of `table_parent`; a decision on the spot for a child
  /// with no table reads the record its own entry keeps.
  fn apply_directive(
    &mut self,
    table_parent: Option<&ActorRef>,
    failure: &Failure,
    directive: Directive,
    strategy: &SupervisorStrategy,
    governing_all_for_one: bool,
    cause: RestartCause,
  ) {
    let child = &failure.child;
    if governing_all_for_one {
      let parent = table_parent.expect("an all-for-one decision is made from a parent's table");
      let granted = match directive {
        | Directive::Restart => match strategy.restart_permission() {
          | Some((limit, window)) => {
            let now = if window.is_some() && !limit.forbids_restart() { self.now() } else { Duration::ZERO };
            let mut granted = true;
            let siblings = self.children_of(parent);
            for sibling in &siblings {
              let mut history = self
                .parent_entry(parent)
                .and_then(|entry| entry.child_record(sibling))
                .map(|record| record.history)
                .unwrap_or_default();
              let allowed = history.request_permission(limit, window, now);
              if let Some(entry) = self.parent_entry_mut(parent) {
                if let Some(record) = entry.child_record_mut(sibling) {
                  record.history = history;
                }
              }
              if !allowed {
                granted = false;
                break;
              }
            }
            granted
          },
          | None => true,
        },
        | Directive::Stop => false,
      };
      if granted {
        for sibling in self.children_of(parent) {
          if &sibling != child {
            self.send_system(&sibling, SystemMessage::Suspend);
          }
          self.send_system(&sibling, SystemMessage::Recreate { cause: cause.clone() });
        }
      } else {
        for sibling in self.children_of(parent) {
          self.stop(&sibling);
        }
      }
      return;
    }
    match directive {
      | Directive::Restart => {
        let permitted = match strategy.restart_permission() {
          | Some((limit, window)) => self.request_permission_for(table_parent, child, limit, window),
          | None => {
            if strategy.records_plain_restart() {
              self.record_plain_for(table_parent, child);
            }
            true
          },
        };
        if permitted {
          self.send_system(child, SystemMessage::Recreate { cause });
        } else {
          self.stop(child);
        }
      },
      | Directive::Stop => self.stop(child),
    }
  }

  /// Asks the restart record of `child` for one restart under `limit`
  /// and `window`, and answers whether it is granted.
  fn request_permission_for(
    &mut self,
    table_parent: Option<&ActorRef>,
    child: &ActorRef,
    limit: RestartLimit,
    window: Option<Duration>,
  ) -> bool {
    match table_parent {
      | Some(parent) => {
        let mut history = self
          .parent_entry(parent)
          .and_then(|entry| entry.child_record(child))
          .map(|record| record.history)
          .unwrap_or_default();
        let allowed = self.apply_permission_to(&mut history, limit, window);
        if let Some(entry) = self.parent_entry_mut(parent) {
          if let Some(record) = entry.child_record_mut(child) {
            record.history = history;
          }
        }
        allowed
      },
      | None => {
        let mut history = self.entry(child).map_or_else(RestartHistory::new, |entry| entry.restart_history);
        let allowed = self.apply_permission_to(&mut history, limit, window);
        if let Some(entry) = self.entry_mut(child) {
          entry.restart_history = history;
        }
        allowed
      },
    }
  }

  /// Records one plain restart request of `child` against its record.
  fn record_plain_for(&mut self, table_parent: Option<&ActorRef>, child: &ActorRef) {
    match table_parent {
      | Some(parent) => {
        if let Some(entry) = self.parent_entry_mut(parent) {
          if let Some(record) = entry.child_record_mut(child) {
            record.history.record_plain();
          }
        }
      },
      | None => {
        if let Some(entry) = self.entry_mut(child) {
          entry.restart_history.record_plain();
        }
      },
    }
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

  /// Handles `Suspend` in the run of `cell` (Pekko's `faultSuspend`):
  /// the actor suspends itself and passes the suspend on to its
  /// children.
  fn fault_suspend(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>) {
    mailbox.suspend();
    let self_ref = cell.self_ref();
    for child in self.children_of(&self_ref) {
      self.send_system(&child, SystemMessage::Suspend);
    }
  }

  /// Handles `Resume` in the run of `cell` (Pekko's `faultResume`).
  ///
  /// An actor without an instance goes to the retry of its start. An
  /// actor with one undoes one suspend, and passes the resume on to its
  /// children: a resume that answers a failure clears the actor's
  /// failure record, and only the child that record names is resumed
  /// with the cause, the others without it.
  fn fault_resume(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>, caused_by_failure: bool) {
    if !cell.is_created() {
      self.fault_create(cell, mailbox);
      return;
    }
    let self_ref = cell.self_ref();
    mailbox.resume();
    let perpetrator =
      if caused_by_failure { self.entry_mut(&self_ref).and_then(|entry| entry.perpetrator.take()) } else { None };
    for child in self.children_of(&self_ref) {
      let flag = caused_by_failure && perpetrator.as_ref() == Some(&child);
      self.send_system(&child, SystemMessage::Resume { caused_by_failure: flag });
    }
  }

  /// Retries the start of `cell`, which has no instance (Pekko's
  /// `faultCreate`): the children of the failed attempt are asked to
  /// stop and waited for when there are any, and the start runs once
  /// they have gone.
  fn fault_create(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>) {
    let self_ref = cell.self_ref();
    if self.stop_children_and_wait(&self_ref, WaitReason::Creation) {
      return;
    }
    self.finish_create(cell, mailbox);
  }

  /// Runs the start of `cell` whose children of the failed attempt have
  /// gone (Pekko's `finishCreate`): one suspend is undone, the failure
  /// record is cleared, and the instance is built.
  fn finish_create(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>) {
    let self_ref = cell.self_ref();
    cell.take_start_failure();
    mailbox.resume();
    if let Some(entry) = self.entry_mut(&self_ref) {
      entry.perpetrator = None;
    }
    self.create_now(cell, mailbox);
  }

  /// Runs the first half of a restart of `cell` in its own run (Pekko's
  /// `faultRecreate`), and the second half at once when no child is to
  /// die.
  ///
  /// A `Recreate` answers a suspend, so the mailbox is suspended when
  /// this runs. An actor without an instance goes to the retry of its
  /// start instead. The children the restart keeps are recorded before
  /// anything runs, so a child the old instance starts in its
  /// `pre_restart` is not among them, while a child the wait itself
  /// brings in joins them. The stash goes back to the mailbox, as
  /// Pekko's `UnrestrictedStash.preRestart` returns it before the old
  /// instance's `pre_restart`, so the new instance handles it.
  /// `pre_restart` runs on the old instance, under the catcher, with the
  /// sender of the failed message as its `sender()`; its failure is
  /// logged and the restart goes on. The children it asked to stop are
  /// waited for: the second half runs in the run that handles the last
  /// one's `ChildTerminated`.
  fn fault_recreate(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>, cause: RestartCause) {
    debug_assert!(mailbox.is_suspended(), "a Recreate answers a suspend");
    if !cell.is_created() {
      self.fault_create(cell, mailbox);
      return;
    }
    let target = cell.self_ref();
    if let Some(entry) = self.parent_entry_mut(&target) {
      entry.pending_survivors = Some(entry.children.iter().map(|record| record.child.clone()).collect());
    }
    self.unstash_all(&target);
    let reason = self.failure_reason(&target, cause.cause_name, &cause.message);
    let reason_box: Box<dyn core::any::Any + Send> = Box::new(reason);
    let catcher = self.catcher.clone();
    let mut old_stack = cell.take_stack();
    self.enter_frame(&target);
    let hook_sender = cell.failed_sender().unwrap_or_else(|| target.clone());
    let pre_restart_failed = {
      let mut context = ActorContext::new(&target, hook_sender, self, &mut old_stack);
      match catcher {
        | Some(catcher) => catcher.run(&mut || cell.pre_restart(&mut context, reason_box.as_ref())),
        | None => {
          cell.pre_restart(&mut context, reason_box.as_ref());
          RunOutcome::Completed
        },
      }
    };
    self.exit_frame();
    if let RunOutcome::Failed { message } = pre_restart_failed {
      self.publish_child_failure(&target, RUNTIME_CAUSE, &message);
    }
    // The old stack lives until the second half has run, as the old
    // instance's fields did across Pekko's restart.
    cell.restore_stack(old_stack);
    let waits = self.parent_entry(&target).is_some_and(|entry| !entry.dying.is_empty());
    if waits {
      if let Some(entry) = self.parent_entry_mut(&target) {
        entry.waiting = Some(WaitReason::Recreation(cause));
      }
    } else {
      self.finish_recreate(cell, mailbox, cause);
    }
  }

  /// Runs the second half of a restart of `cell` (Pekko's
  /// `finishRecreate`).
  ///
  /// One suspend is undone and the failure record is cleared before the
  /// new instance is built, so a child the new instance starts in its
  /// `post_restart` does not take the answered suspend on itself. The
  /// recipe creates the new instance and its `post_restart` runs, which
  /// is what Pekko's default `postRestart` does. The children the
  /// restart kept are restarted with the same cause. A failure of the
  /// recipe or of `post_restart` stops the actor in this run, and the
  /// stop skips the old instance's `post_stop`, which `pre_restart`
  /// already ran.
  fn finish_recreate(&mut self, cell: &mut ActorCell, mailbox: &Mailbox<ActorCell>, cause: RestartCause) {
    let target = cell.self_ref();
    let pending = self.parent_entry_mut(&target).and_then(|entry| entry.pending_survivors.take());
    let survivors: Vec<ActorRef> =
      pending.unwrap_or_default().into_iter().filter(|child| self.is_child_in_table(&target, child)).collect();
    self.resume_mailbox(&target);
    if let Some(entry) = self.entry_mut(&target) {
      entry.perpetrator = None;
    }
    let old_stack = cell.take_stack();
    cell.set_failed_sender(None);
    let catcher = self.catcher.clone();
    let mut stack = BehaviorStack::new();
    self.enter_frame(&target);
    let restarted = {
      let mut context = ActorContext::new(&target, target.clone(), self, &mut stack);
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
        self.check_receive_timeout(target.key(), true);
        for child in survivors {
          self.send_system(&child, SystemMessage::Recreate { cause: cause.clone() });
        }
      },
      | RunOutcome::Failed { message } => {
        self.publish_child_failure(&target, RUNTIME_CAUSE, &message);
        cell.suppress_post_stop();
        if let Some(parent) = cell.parent() {
          if let Some(entry) = self.parent_entry_mut(&parent) {
            entry.mark_dying(&target);
          }
        }
        self.handle_terminate(cell, mailbox);
      },
    }
    drop(old_stack);
  }

  /// Builds an `Error` event with `cause_name` and `message`, raised by
  /// `target`.
  fn failure_reason(&self, target: &ActorRef, cause_name: &str, message: &str) -> Error {
    if cause_name == ACTOR_INITIALIZATION_CAUSE {
      let root = if message.is_empty() { "No message available" } else { message };
      let cause_message =
        alloc::format!("{}: exception during creation, root cause message: [{}]", target.path(), root);
      return Error::with_cause_and_message(
        cause_name.to_string(),
        cause_message,
        target.path().to_string(),
        ACTOR_LOG_CLASS,
        message,
      );
    }
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
        // away. Route the notice itself to dead letters here: the pending
        // record is consumed (never registered) and the internal carrier
        // never surfaces as a dead letter's payload. A custom notice is
        // dropped rather than routed: the notice is a `Terminated`, which
        // the routing rules suppress, so no log line names a text the
        // watcher never asked to have written for a delivery that failed.
        let message = Message::new(notice);
        self.route_undeliverable(message, sender, recipient);
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
    if !self.returned && (!executor.running.is_empty() || executor.in_run) {
      executor.mark_fatal();
    }
  }
}

impl MailboxCell for ActorCell {
  type Env = Executor;

  fn invoke(&mut self, executor: &mut Executor, mailbox: &Mailbox<Self>, envelope: Envelope) -> Next {
    executor.invoke(self, mailbox, envelope)
  }

  fn system_invoke(&mut self, executor: &mut Executor, mailbox: &Mailbox<Self>, message: SystemMessage) {
    executor.system_invoke(self, mailbox, message);
  }
}
