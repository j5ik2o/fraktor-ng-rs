//! The executor that owns one shard of actors and runs their messages.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use core::time::Duration;

use crate::kernel::actor::ActorContext;
use crate::kernel::actor::Props;
use crate::kernel::actor::Terminated;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ActorCell;
use crate::kernel::executor::ShardQueue;
use crate::kernel::message::Envelope;
use crate::kernel::supervision::Directive;
use crate::kernel::supervision::FailureCatcher;
use crate::kernel::supervision::SupervisorStrategy;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_SPAWN: AtomicU64 = AtomicU64::new(0);

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
      terminated: false,
    }
  }

  pub(crate) fn function_ref<H>(&mut self, name: &str, handler: H) -> ActorRef
  where
    H: Fn(&ActorRef, crate::kernel::message::Message) + Send + Sync + 'static,
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
    let cell = ActorCell::new(props.create_actor(), self_ref.clone(), parent, props, strategy);
    self.cells.push(cell);
    self_ref
  }

  /// Stops the actor or function reference `target`, telling every watcher that
  /// it is gone.
  ///
  /// Mirrors `org.apache.pekko.actor.ActorSystem.stop` together with
  /// `ActorContext.stop`: the children stop first, then the cell is removed,
  /// and each registered watcher receives `Terminated(target,
  /// existence_confirmed = true, address_terminated = false)`. Pekko stops the
  /// children before it finishes the parent's termination, so a child that is
  /// watched learns about its own end before the parent's watcher learns about
  /// the parent's.
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
      self.cells.remove(index);
    }
    self.notify_watchers(target, true, false);
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
    while self.queue.boundary().poll().is_some() {}
  }

  /// Returns whether the system has been terminated.
  pub(crate) fn is_terminated(&self) -> bool {
    self.terminated
  }

  /// Refuses to start an actor once the system has been terminated.
  fn assert_running(&self) {
    assert!(!self.terminated, "{TERMINATED_MESSAGE}");
  }

  /// Returns whether `target` can still be watched for its end.
  fn is_live(&self, target: &ActorRef) -> bool {
    self.cells.iter().any(|cell| cell.accepts(target)) || self.function_refs.contains(target)
  }

  fn deliver(&mut self, envelope: Envelope) {
    let (recipient, message, sender) = envelope.into_parts();
    let Some(index) = self.cells.iter().position(|cell| cell.accepts(&recipient)) else {
      return;
    };
    let mut context = ActorContext::new(recipient.clone(), sender, &mut self.queue);
    match self.catcher.clone() {
      | Some(catcher) => {
        let cell = &mut self.cells[index];
        let failed = !catcher.run(&mut || cell.deliver(&mut context, message.clone()));
        if failed {
          self.handle_failure(index, &recipient, &catcher);
        }
      },
      | None => {
        let cell = &mut self.cells[index];
        cell.deliver(&mut context, message);
      },
    }
  }

  /// Applies the supervisor's decision to a failed actor.
  ///
  /// A restart re-creates the actor from its recipe and runs `postRestart`,
  /// either of which may fail. Pekko catches such a failure and stops the
  /// actor with a `PostRestartException`, which the default decider maps to
  /// `stop`; so does this port. Letting the panic out of the executor instead
  /// would stop the whole shard.
  ///
  /// `catcher` is the one the caller already ran the failing message under, so
  /// this path never picks a different catcher or falls back to letting a panic
  /// escape.
  fn handle_failure(&mut self, index: usize, target: &ActorRef, catcher: &Arc<dyn FailureCatcher>) {
    match self.cells[index].strategy().decide() {
      | Directive::Restart => {
        let restarted = catcher.run(&mut || self.cells[index].restart());
        if !restarted {
          self.stop(target);
        }
      },
      | Directive::Stop => self.stop(target),
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
    let envelope = Envelope::new(recipient.clone(), crate::kernel::message::Message::new(message), sender);
    match envelope.into_local_if_same_shard(self.queue.boundary()) {
      | Ok(local) => self.queue.enqueue_locally(local),
      | Err(remote) => remote.post_to_boundary(),
    }
  }
}
