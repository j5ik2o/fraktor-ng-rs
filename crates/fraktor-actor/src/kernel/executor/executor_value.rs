//! The executor that owns one shard of actors and runs their messages.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::time::Duration;

use crate::kernel::actor::ActorContext;
use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ActorCell;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;

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
  pub(crate) fn spawn(&mut self, props: Props) -> ActorRef {
    let id = self.cells.len() as u64;
    let self_ref = ActorRef::new(id, 0, self.boundary());
    let cell = ActorCell::new(props.create_actor(), self_ref.clone());
    self.cells.push(cell);
    self_ref
  }

  /// Delivers every message that is waiting, whether it is in the shard or at
  /// its boundary, until nothing is left.
  pub(crate) fn run_until_idle(&mut self) {
    loop {
      self.drain_boundary();
      match self.local_queue.pop_front() {
        | Some(envelope) => self.deliver(envelope),
        | None => break,
      }
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
  pub(crate) fn stop_all(&mut self) {
    self.cells.clear();
    self.local_queue.clear();
    while self.boundary.poll().is_some() {}
  }

  fn drain_boundary(&mut self) {
    while let Some(envelope) = self.boundary.poll() {
      self.local_queue.push_back(envelope);
    }
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
