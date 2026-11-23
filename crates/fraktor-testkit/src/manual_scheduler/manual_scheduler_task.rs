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

  /// Cancels the registration and returns whether it was still pending.
  ///
  /// Returns true when the registration had not run to completion (for a
  /// one-shot registration) or been cancelled yet. A second call, a call after
  /// the registration ran, or a call after the scheduler was dropped returns
  /// false.
  pub fn cancel(&self) -> bool {
    match self.shared.upgrade() {
      | Some(shared) => shared.remove(self.id),
      | None => false,
    }
  }
}
