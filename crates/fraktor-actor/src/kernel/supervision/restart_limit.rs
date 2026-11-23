//! The maximum number of restart requests a child may receive.

/// A finite restart allowance, including zero to forbid every restart.
#[derive(Clone, Copy)]
pub(crate) struct RestartLimit(u32);

impl RestartLimit {
  /// Creates a finite allowance. Every unsigned value is valid; zero denies
  /// the first request and positive values allow that many requests.
  pub(crate) fn new(maximum: u32) -> Self {
    Self(maximum)
  }

  /// Returns whether the limit forbids every restart.
  pub(crate) fn forbids_restart(self) -> bool {
    self.0 == 0
  }

  /// Returns whether the number of requests is within this allowance.
  pub(crate) fn permits(self, requests: u32) -> bool {
    requests <= self.0
  }
}

impl core::fmt::Debug for RestartLimit {
  fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    core::fmt::Debug::fmt(&self.0, formatter)
  }
}
