//! The maximum number of restart requests a child may receive.

/// A restart allowance: a finite maximum, including zero to forbid every
/// restart, or an omitted maximum that allows every restart.
///
/// Mirrors Pekko's `maxNrOfRetriesOption`: a negative `maxNrOfRetries` is
/// omitted and every request is allowed, otherwise the value caps the requests.
#[derive(Clone, Copy)]
pub(crate) struct RestartLimit(Option<u32>);

impl RestartLimit {
  /// Creates a finite allowance. Every unsigned value is valid; zero denies
  /// the first request and positive values allow that many requests.
  pub(crate) fn new(maximum: u32) -> Self {
    Self(Some(maximum))
  }

  /// Creates an omitted allowance that admits every restart request, as an
  /// absent `maxNrOfRetries` does.
  pub(crate) fn unlimited() -> Self {
    Self(None)
  }

  /// Returns whether the limit forbids every restart.
  pub(crate) fn forbids_restart(self) -> bool {
    self.0 == Some(0)
  }

  /// Returns whether the number of requests is within this allowance. An
  /// omitted maximum permits every count.
  pub(crate) fn permits(self, requests: u32) -> bool {
    match self.0 {
      | Some(maximum) => requests <= maximum,
      | None => true,
    }
  }

  /// Returns the allowance Pekko's window algorithm applies inside a time
  /// range. Mirrors `retriesInWindowOkay(if (x.isDefined) x.get else 1, window)`:
  /// an omitted maximum becomes a single allowed restart per window.
  pub(crate) fn for_window(self) -> Self {
    match self.0 {
      | Some(maximum) => Self::new(maximum),
      | None => Self::new(1),
    }
  }
}

impl core::fmt::Debug for RestartLimit {
  fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    core::fmt::Debug::fmt(&self.0, formatter)
  }
}
