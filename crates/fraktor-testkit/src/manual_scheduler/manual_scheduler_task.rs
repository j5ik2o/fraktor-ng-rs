//! The cancel handle a scheduling call returns.

use std::sync::Weak;

use super::registration_id::RegistrationId;
use super::shared_state::SharedState;

/// Cancels one registration of a [`ManualScheduler`].
///
/// A task is returned by `schedule` and `schedule_once` and cancels exactly the
/// registration it came from, regardless of how many registrations share the
/// same callback. It reaches the scheduler through a weak handle, so it does not
/// keep the scheduler alive; once the scheduler is dropped, cancelling reports
/// that nothing was cancelled.
///
/// [`ManualScheduler`]: super::ManualScheduler
pub struct ManualSchedulerTask {
  shared: Weak<SharedState>,
  id: RegistrationId,
}

impl ManualSchedulerTask {
  /// Creates a task that cancels registration `id` in `shared`.
  pub(crate) fn new(shared: Weak<SharedState>, id: RegistrationId) -> Self {
    Self { shared, id }
  }

  /// Cancels the registration and returns whether it had any work left to cancel.
  ///
  /// A pending registration can be cancelled. Cancelling a running repeat
  /// stops subsequent executions. Cancelling a completed or already cancelled
  /// registration, or cancelling after the scheduler is dropped, returns false.
  pub fn cancel(&self) -> bool {
    match self.shared.upgrade() {
      | Some(shared) => shared.remove(self.id),
      | None => false,
    }
  }
}
