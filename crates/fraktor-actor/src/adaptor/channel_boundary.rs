//! A shard boundary backed by a standard-library channel.

use core::time::Duration;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::sync::Mutex;

use crate::adaptor::CondvarSignal;
use crate::adaptor::SystemClock;
use crate::kernel::dispatch::WakeSignal;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::Clock;

#[cfg(test)]
#[path = "channel_boundary_test.rs"]
mod tests;

/// Carries the messages that have no mailbox to go to over a
/// standard-library channel.
///
/// The boundary also owns the signal that wakes a thread waiting for work for
/// its system: a post raises it, and so do the system's mailboxes and
/// function references. The receiving end needs `&self` to poll it,
/// so it sits behind a `Mutex`; a poisoned lock still yields its data, because
/// a panic while sending must not lose the messages already queued.
///
/// A counter of the envelopes that were posted but not yet taken answers
/// `has_pending`. It is raised inside the lock that guards `open`, so a post
/// that observes an open boundary and the close that follows cannot interleave
/// and leave a stale count.
pub struct ChannelBoundary {
  sender: Sender<Envelope>,
  receiver: Mutex<Receiver<Envelope>>,
  open: Mutex<bool>,
  pending: AtomicUsize,
  clock: SystemClock,
  signal: Arc<CondvarSignal>,
}

impl ChannelBoundary {
  /// Creates a boundary whose two ends are held by this one object.
  pub fn new() -> Self {
    let (sender, receiver) = mpsc::channel();
    Self {
      sender,
      receiver: Mutex::new(receiver),
      open: Mutex::new(true),
      pending: AtomicUsize::new(0),
      clock: SystemClock::new(),
      signal: Arc::new(CondvarSignal::new()),
    }
  }

  fn lock_receiver(&self) -> std::sync::MutexGuard<'_, Receiver<Envelope>> {
    self.receiver.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  fn lock_open(&self) -> std::sync::MutexGuard<'_, bool> {
    self.open.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  fn take(&self, envelope: Option<Envelope>) -> Option<Envelope> {
    if envelope.is_some() {
      self.pending.fetch_sub(1, Ordering::AcqRel);
    }
    envelope
  }
}

impl Default for ChannelBoundary {
  fn default() -> Self {
    Self::new()
  }
}

impl ShardBoundary for ChannelBoundary {
  fn monotonic_time(&self) -> Option<Duration> {
    Some(self.clock.monotonic())
  }

  fn post(&self, envelope: Envelope) {
    let open = self.lock_open();
    if !*open {
      return;
    }
    self.pending.fetch_add(1, Ordering::AcqRel);
    self.sender.send(envelope).ok();
    drop(open);
    self.signal.notify();
  }

  fn poll(&self) -> Option<Envelope> {
    let envelope = self.lock_receiver().try_recv().ok();
    self.take(envelope)
  }

  fn poll_within(&self, max: Duration) -> Option<Envelope> {
    let envelope = self.lock_receiver().recv_timeout(max).ok();
    self.take(envelope)
  }

  fn has_pending(&self) -> bool {
    self.pending.load(Ordering::Acquire) > 0
  }

  fn wake_signal(&self) -> Option<Arc<dyn WakeSignal>> {
    let signal: Arc<dyn WakeSignal> = self.signal.clone();
    Some(signal)
  }

  fn close(&self) {
    *self.lock_open() = false;
  }
}
