//! The identifier one registration carries for as long as it lives.

/// Identifies one registration in a scheduler's table.
///
/// Each call to `schedule` or `schedule_once` takes the next value, so two
/// registrations of the same callback never share an identifier. The value is
/// internal: a test reaches a registration through the [`ManualSchedulerTask`]
/// that scheduling returned.
///
/// [`ManualSchedulerTask`]: super::ManualSchedulerTask
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub(crate) struct RegistrationId(u64);

impl RegistrationId {
  /// Returns the identifier that follows `self`.
  pub(crate) const fn next(self) -> Self {
    Self(self.0 + 1)
  }
}
