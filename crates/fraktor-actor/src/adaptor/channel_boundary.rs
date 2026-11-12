//! A shard boundary backed by a standard-library channel.

use core::time::Duration;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;

#[cfg(test)]
#[path = "channel_boundary_test.rs"]
mod tests;

/// Carries messages between shards over a standard-library channel.
///
/// The temporary policy of G0b puts a channel only at the edge of a shard, and
/// this is its one implementation. The receiving end needs `&self` to poll it,
/// so it sits behind a `Mutex`; a poisoned lock still yields its data, because
/// a panic while sending must not lose the messages already queued.
pub struct ChannelBoundary {
  sender: Sender<Envelope>,
  receiver: Mutex<Receiver<Envelope>>,
  open: Mutex<bool>,
}

impl ChannelBoundary {
  /// Creates a boundary whose two ends are held by this one object.
  pub fn new() -> Self {
    let (sender, receiver) = mpsc::channel();
    Self { sender, receiver: Mutex::new(receiver), open: Mutex::new(true) }
  }

  fn lock_receiver(&self) -> std::sync::MutexGuard<'_, Receiver<Envelope>> {
    self.receiver.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  fn lock_open(&self) -> std::sync::MutexGuard<'_, bool> {
    self.open.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

impl Default for ChannelBoundary {
  fn default() -> Self {
    Self::new()
  }
}

impl ShardBoundary for ChannelBoundary {
  fn post(&self, envelope: Envelope) {
    if !*self.lock_open() {
      return;
    }
    let _ = self.sender.send(envelope);
  }

  fn poll(&self) -> Option<Envelope> {
    self.lock_receiver().try_recv().ok()
  }

  fn poll_within(&self, max: Duration) -> Option<Envelope> {
    self.lock_receiver().recv_timeout(max).ok()
  }

  fn close(&self) {
    *self.lock_open() = false;
  }
}
