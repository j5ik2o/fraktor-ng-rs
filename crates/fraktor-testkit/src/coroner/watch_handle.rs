//! The handle that cancels a Coroner watch.

use core::time::Duration;

use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::MutexGuard;

/// Cancels a [`watch`](super::watch) and waits for its outcome.
///
/// Mirrors `Coroner.WatchHandle`. The outcome is `true` once the watch was
/// cancelled and `false` once it expired; Pekko completes a promise with the
/// first of the two, and so does the handle. A clone shares the watch.
#[derive(Clone)]
pub struct WatchHandle {
  state: Arc<WatchState>,
  start_and_stop: Duration,
}

/// What the watching thread and the test share.
struct WatchState {
  progress: Mutex<Progress>,
  changed: Condvar,
}

/// The outcome of the watch and how far its thread has got.
#[derive(Default)]
struct Progress {
  outcome: Option<bool>,
  started: bool,
  finished: bool,
}

impl WatchHandle {
  pub(super) fn new(start_and_stop: Duration) -> Self {
    let state = WatchState { progress: Mutex::new(Progress::default()), changed: Condvar::new() };
    Self { state: Arc::new(state), start_and_stop }
  }

  /// Cancels the watch and waits for its thread to finish.
  ///
  /// Mirrors `WatchHandle.cancel`: the outcome becomes `true` unless the watch
  /// has already expired, and the call then waits at most the start-and-stop
  /// duration for the thread to finish writing.
  pub fn cancel(&self) {
    let mut progress = self.lock();
    progress.outcome.get_or_insert(true);
    self.state.changed.notify_all();
    let _unused = self
      .state
      .changed
      .wait_timeout_while(progress, self.start_and_stop, |progress| !progress.finished)
      .unwrap_or_else(|poisoned| poisoned.into_inner());
  }

  /// Waits at most `max` for the watch to be cancelled or to expire, and
  /// returns the handle, as `Await.ready` does.
  pub fn ready(&self, max: Duration) -> &Self {
    let _ = self.result(max);
    self
  }

  /// Waits at most `max` for the watch to be cancelled or to expire.
  ///
  /// Mirrors `WatchHandle.result`: returns `true` when it was cancelled, and
  /// `false` when it expired or when `max` passed first.
  pub fn result(&self, max: Duration) -> bool {
    let progress = self.lock();
    let (progress, _) = self
      .state
      .changed
      .wait_timeout_while(progress, max, |progress| progress.outcome.is_none())
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    progress.outcome.unwrap_or(false)
  }

  /// Waits at most the start-and-stop duration for the thread to start.
  pub(super) fn wait_for_start(&self) {
    let progress = self.lock();
    let _unused = self
      .state
      .changed
      .wait_timeout_while(progress, self.start_and_stop, |progress| !progress.started)
      .unwrap_or_else(|poisoned| poisoned.into_inner());
  }

  /// Records that the watching thread has started.
  pub(super) fn started(&self) {
    self.lock().started = true;
    self.state.changed.notify_all();
  }

  /// Waits at most `duration` for a cancel. Returns `true` when none came, in
  /// which case the outcome is now `false`.
  pub(super) fn expire_unless_cancelled(&self, duration: Duration) -> bool {
    let progress = self.lock();
    let (mut progress, _) = self
      .state
      .changed
      .wait_timeout_while(progress, duration, |progress| progress.outcome.is_none())
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if progress.outcome.is_some() {
      return false;
    }
    progress.outcome = Some(false);
    self.state.changed.notify_all();
    true
  }

  /// Records that the watching thread has written everything it will write.
  pub(super) fn finished(&self) {
    self.lock().finished = true;
    self.state.changed.notify_all();
  }

  fn lock(&self) -> MutexGuard<'_, Progress> {
    self.state.progress.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}
