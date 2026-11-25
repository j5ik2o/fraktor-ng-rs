//! A count-down latch whose waits run the actor system and use the time factor.

use core::time::Duration;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::time::Instant;

use fraktor_actor::kernel::actor_system::ActorSystem;

use crate::test_kit_extension::TestKitSettings;

#[cfg(test)]
#[path = "test_latch_test.rs"]
mod tests;

/// The longest a waiting `ready` sleeps at the boundary before it checks the
/// latch again, so a count-down from another thread is seen promptly.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// A count-down latch for tests.
///
/// Mirrors `org.apache.pekko.testkit.TestLatch`: it opens once it has been
/// counted down `count` times, `open` opens it at once, `reset` closes it again
/// with the original count, and a wait always has a timeout that is scaled by
/// the test time factor and fails the test when it passes.
///
/// A clone shares the count, so a test can hand a clone to an actor or a thread
/// and wait on the original. Pekko waits with `Await.ready(latch, max)` while
/// the actors run on their dispatchers; the port's actors run when the test
/// thread drives the system, so [`TestLatch::ready`] takes the system and
/// keeps delivering its messages while it waits.
#[derive(Clone)]
pub struct TestLatch {
  remaining: Arc<Mutex<usize>>,
  count: usize,
  settings: TestKitSettings,
}

impl TestLatch {
  /// The timeout Pekko names `TestLatch.DefaultTimeout`.
  pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

  /// Creates a closed latch that opens after `count` count-downs and dilates
  /// its waits with the time factor of `settings`.
  ///
  /// Pekko takes the actor system to read the time factor from its test kit
  /// extension; the port takes the settings, which is where a spec built with
  /// a time factor keeps it (`FraktorSpec::test_kit_settings`).
  pub fn new(count: usize, settings: TestKitSettings) -> Self {
    Self { remaining: Arc::new(Mutex::new(count)), count, settings }
  }

  /// Counts the latch down by one. Counting down an open latch does nothing.
  pub fn count_down(&self) {
    let mut remaining = self.lock();
    *remaining = remaining.saturating_sub(1);
  }

  /// Returns whether the latch has been counted down to zero.
  pub fn is_open(&self) -> bool {
    *self.lock() == 0
  }

  /// Opens the latch at once.
  pub fn open(&self) {
    *self.lock() = 0;
  }

  /// Closes the latch again with the count it was created with.
  pub fn reset(&self) {
    *self.lock() = self.count;
  }

  /// Waits until the latch is open, delivering the messages of `system` while
  /// it waits.
  ///
  /// Mirrors `TestLatch.ready`: `max` is dilated by the time factor, and the
  /// wait fails with `Timeout of <max> with time factor of <factor>` when the
  /// latch is still closed at the deadline. The latch is checked before the
  /// first delivery, so an open latch returns at once, also on a terminated
  /// system.
  ///
  /// # Panics
  ///
  /// Panics when the latch is not open within the dilated `max`.
  pub fn ready(&self, system: &ActorSystem, max: Duration) {
    let system = system.downgrade();
    let deadline = Instant::now() + self.settings.dilated(max);
    loop {
      if self.is_open() {
        return;
      }
      let left = deadline.saturating_duration_since(Instant::now());
      assert!(left > Duration::ZERO, "Timeout of {max:?} with time factor of {}", self.settings.test_time_factor());
      if !system.run_one() {
        system.await_envelope(left.min(POLL_INTERVAL));
      }
    }
  }

  fn lock(&self) -> MutexGuard<'_, usize> {
    self.remaining.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}
