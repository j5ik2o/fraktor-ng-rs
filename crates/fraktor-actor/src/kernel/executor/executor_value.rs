//! The executor that owns one shard of actors and runs their messages.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use core::time::Duration;

use crate::kernel::actor::ActorContext;
use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ActorCell;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Creates actors of one shard, runs their messages, and owns their state.
///
/// Mirrors the role of `org.apache.pekko.actor.ActorCell` together with a
/// dispatcher, reduced to the temporary policy of G0b: one shard per executor,
/// a local queue inside the shard, and a `ShardBoundary` only at the edge. The
/// executor alone owns the cells; a caller only holds `ActorRef` handles.
pub(crate) struct Executor {
  cells: Vec<ActorCell>,
  local_queue: VecDeque<Envelope>,
  boundary: Arc<dyn ShardBoundary>,
}

impl Executor {
  pub(crate) fn new(boundary: Arc<dyn ShardBoundary>) -> Self {
    Self { cells: Vec::new(), local_queue: VecDeque::new(), boundary }
  }

  pub(crate) fn boundary(&self) -> Arc<dyn ShardBoundary> {
    Arc::clone(&self.boundary)
  }

  /// Starts the actor a recipe describes and returns its handle.
  ///
  /// The identifier comes from a counter shared by every executor, so handles
  /// of actors in different systems never collide, as Pekko's per-actor uid
  /// keeps them apart.
  pub(crate) fn spawn(&mut self, props: Props) -> ActorRef {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let self_ref = ActorRef::new(id, 0, self.boundary());
    let cell = ActorCell::new(props.create_actor(), self_ref.clone());
    self.cells.push(cell);
    self_ref
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
    if let Some(envelope) = self.boundary.poll() {
      self.local_queue.push_back(envelope);
    }
    match self.local_queue.pop_front() {
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
    match self.boundary.poll_within(max) {
      | Some(envelope) => {
        self.local_queue.push_back(envelope);
        true
      },
      | None => false,
    }
  }

  /// Drops the actors and every queued message.
  ///
  /// The boundary is closed first, so an `ActorRef` that is still around cannot
  /// put a message back into it after it has been drained; otherwise the queued
  /// envelope would keep the boundary alive through the handle's `Arc` and the
  /// system would never be released.
  pub(crate) fn stop_all(&mut self) {
    self.boundary.close();
    self.cells.clear();
    self.local_queue.clear();
    while self.boundary.poll().is_some() {}
  }

  fn deliver(&mut self, envelope: Envelope) {
    let (recipient, message, sender) = envelope.into_parts();
    let Some(index) = self.cells.iter().position(|cell| cell.accepts(&recipient)) else {
      return;
    };
    let mut context = ActorContext::new(recipient, sender, Arc::clone(&self.boundary), &mut self.local_queue);
    self.cells[index].deliver(&mut context, message);
  }
}
