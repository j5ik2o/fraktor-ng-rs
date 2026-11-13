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

/// Creates actors of one shard, runs their messages, and owns their state.
///
/// Mirrors the role of `org.apache.pekko.actor.ActorCell` together with a
/// dispatcher, reduced to the temporary policy of this port: one shard per
/// executor, a local queue inside the shard, and a `ShardBoundary` only at the
/// edge. The executor alone owns the cells; a caller only holds `ActorRef`
/// handles.
pub(crate) struct Executor {
  cells: Vec<ActorCell>,
  queue: ShardQueue,
  root: ActorPath,
  watchers: Vec<(ActorRef, ActorRef)>,
  catcher: Option<Arc<dyn FailureCatcher>>,
}

impl Executor {
  pub(crate) fn new(root: ActorPath, boundary: Arc<dyn crate::kernel::executor::ShardBoundary>) -> Self {
    Self { cells: Vec::new(), queue: ShardQueue::new(boundary), root, watchers: Vec::new(), catcher: None }
  }

  pub(crate) fn function_ref<H>(&mut self, name: &str, handler: H) -> ActorRef
  where
    H: Fn(&ActorRef, crate::kernel::message::Message) + Send + Sync + 'static,
  {
    let path = self.root.with_child("system").with_child(name);
    let boundary: Arc<dyn crate::kernel::executor::ShardBoundary> =
      Arc::new(crate::kernel::executor::FunctionBoundary::new(Arc::new(handler)));
    next_actor_ref(path, boundary)
  }

  pub(crate) fn set_failure_catcher(&mut self, catcher: Arc<dyn FailureCatcher>) {
    self.catcher = Some(catcher);
  }

  /// Starts an actor that the system owns directly, below `/user`.
  pub(crate) fn spawn_user(&mut self, props: Props) -> ActorRef {
    let name = alloc::format!("${}", next_spawn_name());
    let path = self.root.with_child("user").with_child(&name);
    self.spawn_at(props, path, SupervisorStrategy::default_strategy())
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
    let label = match name {
      | Some(name) => name.to_string(),
      | None => alloc::format!("${}", next_spawn_name()),
    };
    let path = parent.path().with_child(&label);
    self.spawn_at(props, path, strategy)
  }

  fn spawn_at(&mut self, props: Props, path: ActorPath, strategy: SupervisorStrategy) -> ActorRef {
    let self_ref = next_actor_ref(path, self.queue.boundary().clone());
    let cell = ActorCell::new(props.create_actor(), self_ref.clone(), props, strategy);
    self.cells.push(cell);
    self_ref
  }

  /// Stops the actor `target`, telling every watcher that it is gone.
  ///
  /// Mirrors `org.apache.pekko.actor.ActorSystem.stop` together with
  /// `ActorContext.stop`: the cell is removed, and each registered watcher
  /// receives `Terminated(target, existence_confirmed = true,
  /// address_terminated = false)`.
  pub(crate) fn stop(&mut self, target: &ActorRef) {
    let Some(index) = self.cells.iter().position(|cell| cell.accepts(target)) else {
      return;
    };
    self.cells.remove(index);
    self.notify_watchers(target, true, false);
  }

  /// Registers `watcher` to receive the `Terminated` notice of `target`.
  ///
  /// Mirrors `ActorContext.watch`. When the target is already gone, the
  /// notice arrives at once with `existence_confirmed = false`, so a watcher
  /// learns about a late watch without waiting.
  pub(crate) fn watch(&mut self, watcher: &ActorRef, target: &ActorRef) {
    if self.cells.iter().any(|cell| cell.accepts(target)) {
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
  pub(crate) fn stop_all(&mut self) {
    self.queue.boundary().close();
    self.cells.clear();
    self.queue.clear();
    while self.queue.boundary().poll().is_some() {}
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
          self.handle_failure(index, &recipient);
        }
      },
      | None => {
        let cell = &mut self.cells[index];
        cell.deliver(&mut context, message);
      },
    }
  }

  fn handle_failure(&mut self, index: usize, target: &ActorRef) {
    match self.cells[index].strategy().decide() {
      | Directive::Restart => self.cells[index].restart(),
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
