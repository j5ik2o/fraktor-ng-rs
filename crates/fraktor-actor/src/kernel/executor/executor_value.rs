//! The executor that owns one shard of actors and runs their messages.

use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use core::time::Duration;

use crate::kernel::actor::ActorContext;
use crate::kernel::actor::Logging::Error;
use crate::kernel::actor::Logging::EventSink;
use crate::kernel::actor::Props;
use crate::kernel::actor::Terminated;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ActorCell;
use crate::kernel::executor::ShardQueue;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::supervision::Directive;
use crate::kernel::supervision::FailureCatcher;
use crate::kernel::supervision::FailureKind;
use crate::kernel::supervision::RunOutcome;
use crate::kernel::supervision::SupervisorStrategy;

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

/// A reply to send to `reply_to` after the actor it was registered on stops.
struct DeferredReply {
  reply_to: ActorRef,
  message: Message,
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
  function_refs: Vec<ActorRef>,
  catcher: Option<Arc<dyn FailureCatcher>>,
  event_sink: Option<Arc<dyn EventSink>>,
  deferred_replies: Vec<(ActorRef, DeferredReply)>,
  terminated: bool,
}

impl Executor {
  pub(crate) fn new(root: ActorPath, boundary: Arc<dyn crate::kernel::executor::ShardBoundary>) -> Self {
    Self {
      cells: Vec::new(),
      queue: ShardQueue::new(boundary),
      root,
      watchers: Vec::new(),
      function_refs: Vec::new(),
      catcher: None,
      event_sink: None,
      deferred_replies: Vec::new(),
      terminated: false,
    }
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
    self.event_sink = Some(sink);
  }

  pub(crate) fn event_sink(&self) -> Option<Arc<dyn EventSink>> {
    self.event_sink.clone()
  }

  /// Starts an actor that the system owns directly, below `/user`.
  pub(crate) fn spawn_user(&mut self, props: Props) -> ActorRef {
    self.assert_running();
    let name = alloc::format!("${}", next_spawn_name());
    let path = self.root.with_child("user").with_child(&name);
    self.spawn_at(props, path, None, SupervisorStrategy::default_strategy())
  }

  /// Starts an actor below `parent`, using `name` when one is given.
  ///
  /// Mirrors `ActorContext.actorOf`: the child's path is the parent's path
  /// with the name appended, and a name drawn from a shared counter is used
  /// otherwise.
  pub(crate) fn spawn_child(
    &mut self,
    parent: &ActorRef,
    props: Props,
    name: Option<&str>,
    strategy: SupervisorStrategy,
  ) -> ActorRef {
    self.assert_running();
    let label = match name {
      | Some(name) => name.to_string(),
      | None => alloc::format!("${}", next_spawn_name()),
    };
    let path = parent.path().with_child(&label);
    self.spawn_at(props, path, Some(parent.clone()), strategy)
  }

  fn spawn_at(
    &mut self,
    props: Props,
    path: ActorPath,
    parent: Option<ActorRef>,
    strategy: SupervisorStrategy,
  ) -> ActorRef {
    let self_ref = next_actor_ref(path, self.queue.boundary().clone());
    let cell = ActorCell::pending(self_ref.clone(), parent, props, strategy);
    self.cells.push(cell);
    let index = self.cells.len() - 1;
    self.create_initial(index);
    self_ref
  }

  /// Builds the actor of `index` and runs its `pre_start`, repeating while the
  /// strategy allows a restart on a failed startup and stopping once it does
  /// not.
  ///
  /// Mirrors Pekko's spawn path: a `preStart` that throws is wrapped in an
  /// `ActorInitializationException`, the parent decides whether to restart, and
  /// a child that stops because it never started runs no `postStop` because the
  /// actor was never created.
  fn create_initial(&mut self, index: usize) {
    let self_ref = self.cells[index].self_ref();
    loop {
      let outcome = match self.catcher.clone() {
        | Some(catcher) => catcher.run(&mut || self.cells[index].create()),
        | None => {
          self.cells[index].create();
          RunOutcome::Completed
        },
      };
      match outcome {
        | RunOutcome::Completed => return,
        | RunOutcome::Failed { message } => {
          self.publish_child_failure(&self_ref, ACTOR_INITIALIZATION_CAUSE, &message);
          let strategy = *self.cells[index].strategy();
          let count = self.cells[index].restart_count();
          match strategy.decide(count, FailureKind::Initialization) {
            | Directive::Restart => self.cells[index].record_restart_request(),
            | Directive::Stop => {
              self.stop_as_never_started(&self_ref);
              return;
            },
          }
        },
      }
    }
  }

  /// Stops an actor that never finished starting, without running `postStop`.
  fn stop_as_never_started(&mut self, target: &ActorRef) {
    if let Some(index) = self.cells.iter().position(|cell| cell.accepts(target)) {
      self.cells.remove(index);
    }
    self.notify_watchers(target, true, false);
    self.flush_deferred_replies(target);
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
  pub(crate) fn stop(&mut self, target: &ActorRef) {
    let is_cell = self.cells.iter().any(|cell| cell.accepts(target));
    let is_function_ref = self.function_refs.contains(target);
    if !is_cell && !is_function_ref {
      return;
    }
    let children: Vec<ActorRef> =
      self.cells.iter().filter(|cell| cell.is_child_of(target)).map(|cell| cell.self_ref()).collect();
    for child in children {
      self.stop(&child);
    }
    if let Some(index) = self.function_refs.iter().position(|reference| reference == target) {
      let reference = self.function_refs.remove(index);
      reference.boundary().close();
    }
    if let Some(index) = self.cells.iter().position(|cell| cell.accepts(target)) {
      if !self.cells[index].post_stop_suppressed() {
        self.run_post_stop(index);
      }
      self.cells.remove(index);
    }
    self.notify_watchers(target, true, false);
    self.flush_deferred_replies(target);
  }

  /// Runs `postStop` on the behaviour of `index`, publishing an error when it
  /// throws.
  ///
  /// Mirrors `dungeon.FaultHandling`: a `postStop` that throws is logged as an
  /// error and the stop continues. A failure here never restarts the actor.
  fn run_post_stop(&mut self, index: usize) {
    let self_ref = self.cells[index].self_ref();
    match self.catcher.clone() {
      | Some(catcher) => {
        if let RunOutcome::Failed { message } = catcher.run(&mut || self.cells[index].post_stop()) {
          self.publish_child_failure(&self_ref, EXCEPTION_CAUSE, &message);
        }
      },
      | None => self.cells[index].post_stop(),
    }
  }

  /// Registers a reply to `reply_to` after `target` stops.
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

  /// Registers `watcher` to receive the `Terminated` notice of `target`.
  ///
  /// Mirrors `ActorContext.watch` for the scope this port can serve: actors of
  /// this system, and the function references that this system made for its
  /// test actors. A live target of either kind is registered as a watcher; the
  /// notice comes when it is stopped.
  ///
  /// Anything else is outside that scope: an actor of another system, a
  /// reference that has already stopped, and the temporary reply address that
  /// an `ask` creates. Those are reported at once with `existence_confirmed =
  /// false` and `address_terminated = false`, and no later notice follows. A
  /// target that keeps running out of scope is therefore indistinguishable from
  /// a stopped one, which is the documented limit of this port.
  pub(crate) fn watch(&mut self, watcher: &ActorRef, target: &ActorRef) {
    if self.is_live(target) {
      self.watchers.push((watcher.clone(), target.clone()));
    } else {
      self.enqueue_to(watcher.clone(), Terminated::new(target.clone(), false, false));
    }
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
    self.cells.clear();
    self.queue.clear();
    self.deferred_replies.clear();
    while self.queue.boundary().poll().is_some() {}
  }

  /// Returns whether the system has been terminated.
  pub(crate) fn is_terminated(&self) -> bool {
    self.terminated
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
  fn is_live(&self, target: &ActorRef) -> bool {
    self.cells.iter().any(|cell| cell.accepts(target)) || self.function_refs.contains(target)
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
    let Some(index) = self.cells.iter().position(|cell| cell.accepts(&recipient)) else {
      return;
    };
    if message == KILL {
      self.handle_kill(index, &recipient);
      return;
    }
    if message == POISON_PILL {
      self.stop(&recipient);
      return;
    }
    let mut cell = self.cells.remove(index);
    let delivered = message.clone();
    let catcher = self.catcher.clone();
    let outcome = {
      let mut context = ActorContext::new(recipient.clone(), sender, self);
      match catcher {
        | Some(catcher) => catcher.run(&mut || cell.deliver(&mut context, delivered.clone())),
        | None => {
          cell.deliver(&mut context, delivered);
          RunOutcome::Completed
        },
      }
    };
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
          self.stop(&recipient);
        } else {
          self.apply_failure(cell, &recipient, RUNTIME_CAUSE, &message);
        }
      },
    }
  }

  /// Returns whether the actor asked to stop after the reply it registered.
  fn deferred_stop(&self, target: &ActorRef) -> bool {
    self.deferred_replies.iter().any(|(actor, _)| actor == target)
  }

  /// Applies the supervisor's decision to a failure during `receive`, then
  /// carries out the requests the actor recorded before it failed.
  fn apply_failure(&mut self, mut cell: ActorCell, target: &ActorRef, cause_name: &str, message: &str) {
    self.publish_child_failure(target, cause_name, message);
    let strategy = *cell.strategy();
    let count = cell.restart_count();
    match strategy.decide(count, FailureKind::MessageHandling) {
      | Directive::Restart => {
        cell.record_restart_request();
        self.restart_cell(target, cell, cause_name, message);
      },
      | Directive::Stop => {
        self.cells.push(cell);
        self.stop(target);
      },
    }
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
  fn restart_cell(&mut self, target: &ActorRef, mut cell: ActorCell, cause_name: &str, message: &str) {
    let reason = self.failure_reason(target, cause_name, message);
    let reason_box: Box<dyn core::any::Any + Send> = Box::new(reason);
    let pre_restart_failed = match self.catcher.clone() {
      | Some(catcher) => catcher.run(&mut || cell.pre_restart(reason_box.as_ref())),
      | None => {
        cell.pre_restart(reason_box.as_ref());
        RunOutcome::Completed
      },
    };
    if let RunOutcome::Failed { message } = pre_restart_failed {
      self.publish_child_failure(target, RUNTIME_CAUSE, &message);
    }
    let restarted = match self.catcher.clone() {
      | Some(catcher) => catcher.run(&mut || cell.restart()),
      | None => {
        cell.restart();
        RunOutcome::Completed
      },
    };
    match restarted {
      | RunOutcome::Completed => self.cells.push(cell),
      | RunOutcome::Failed { message } => {
        self.publish_child_failure(target, RUNTIME_CAUSE, &message);
        cell.suppress_post_stop();
        self.cells.push(cell);
        self.stop(target);
      },
    }
  }

  /// Handles a `Kill`: the instance fails with `ActorKilledException` and the
  /// supervisor decides for a killed child, which the default decider maps to
  /// `Stop`.
  fn handle_kill(&mut self, index: usize, recipient: &ActorRef) {
    self.publish_child_failure(recipient, ACTOR_KILLED_CAUSE, ACTOR_KILLED_MESSAGE);
    let mut cell = self.cells.remove(index);
    let strategy = *cell.strategy();
    let count = cell.restart_count();
    match strategy.decide(count, FailureKind::Killed) {
      | Directive::Restart => {
        cell.record_restart_request();
        self.restart_cell(recipient, cell, ACTOR_KILLED_CAUSE, ACTOR_KILLED_MESSAGE);
      },
      | Directive::Stop => {
        self.cells.push(cell);
        self.stop(recipient);
      },
    }
  }

  /// Builds an `Error` event with `cause_name` and `message`, raised by
  /// `target`.
  fn failure_reason(&self, target: &ActorRef, cause_name: &str, message: &str) -> Error {
    Error::with_cause_name(cause_name, target.path().to_string(), ACTOR_LOG_CLASS, message)
  }

  /// Publishes the failure of `target` as an error event.
  ///
  /// Mirrors `FaultHandling.logFailure`: the parent logs an `Error` for a child
  /// it restarts or stops, carrying the path of the child as its source.
  fn publish_child_failure(&self, target: &ActorRef, cause_name: &str, message: &str) {
    if let Some(sink) = self.event_sink.as_ref() {
      let event = self.failure_reason(target, cause_name, message);
      sink.publish(&event);
    }
  }

  fn notify_watchers(&mut self, target: &ActorRef, existence_confirmed: bool, address_terminated: bool) {
    let watchers: Vec<ActorRef> =
      self.watchers.iter().filter(|(_, watched)| watched == target).map(|(watcher, _)| watcher.clone()).collect();
    self.watchers.retain(|(_, watched)| watched != target);
    for watcher in watchers {
      self.enqueue_to(watcher, Terminated::new(target.clone(), existence_confirmed, address_terminated));
    }
  }

  fn enqueue_to(&mut self, recipient: ActorRef, message: Terminated) {
    let sender = recipient.clone();
    let envelope = Envelope::new(recipient.clone(), Message::new(message), sender);
    match envelope.into_local_if_same_shard(self.queue.boundary()) {
      | Ok(local) => self.queue.enqueue_locally(local),
      | Err(remote) => remote.post_to_boundary(),
    }
  }
}
