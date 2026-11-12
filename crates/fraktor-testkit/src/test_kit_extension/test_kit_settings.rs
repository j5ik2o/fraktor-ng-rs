//! The settings that `TestKitExtension` exposes to a test.

use core::time::Duration;

use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::timeout::Timeout;

#[cfg(test)]
#[path = "test_kit_settings_test.rs"]
mod tests;

// Defaults from `testkit/src/main/resources/reference.conf` in Pekko v1.7.1.
const TEST_TIME_FACTOR: f64 = 1.0;
const SINGLE_EXPECT_DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);
const EXPECT_NO_MESSAGE_DEFAULT_TIMEOUT: Duration = Duration::from_millis(100);
const TEST_EVENT_FILTER_LEEWAY: Duration = Duration::from_secs(3);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// The values that `org.apache.pekko.testkit.TestKitSettings` reads from
/// configuration.
///
/// Decision E of G0b holds the reference.conf defaults as constants and builds
/// no configuration system. The values therefore do not depend on the actor
/// system yet, but the constructor takes one so that the call sites mirror
/// `TestKitExtension(system)` and keep a single entry point for both
/// `DefaultTimeout` and `PekkoSpec::test_kit_settings`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TestKitSettings {
  test_time_factor: f64,
  single_expect_default_timeout: Duration,
  expect_no_message_default_timeout: Duration,
  test_event_filter_leeway: Duration,
  default_timeout: Timeout,
}

impl TestKitSettings {
  /// Returns the settings for the given actor system.
  pub fn from_system(_system: &ActorSystem) -> Self {
    Self {
      test_time_factor: TEST_TIME_FACTOR,
      single_expect_default_timeout: SINGLE_EXPECT_DEFAULT_TIMEOUT,
      expect_no_message_default_timeout: EXPECT_NO_MESSAGE_DEFAULT_TIMEOUT,
      test_event_filter_leeway: TEST_EVENT_FILTER_LEEWAY,
      default_timeout: Timeout::new(DEFAULT_TIMEOUT),
    }
  }

  /// Returns `pekko.test.timefactor`.
  pub fn test_time_factor(&self) -> f64 {
    self.test_time_factor
  }

  /// Returns `pekko.test.single-expect-default`.
  pub fn single_expect_default_timeout(&self) -> Duration {
    self.single_expect_default_timeout
  }

  /// Returns `pekko.test.expect-no-message-default`.
  pub fn expect_no_message_default_timeout(&self) -> Duration {
    self.expect_no_message_default_timeout
  }

  /// Returns `pekko.test.filter-leeway`.
  pub fn test_event_filter_leeway(&self) -> Duration {
    self.test_event_filter_leeway
  }

  /// Returns `pekko.test.default-timeout`, the implicit timeout of
  /// `DefaultTimeout`.
  pub fn default_timeout(&self) -> Timeout {
    self.default_timeout
  }

  /// Multiplies `duration` by `pekko.test.timefactor`.
  ///
  /// Mirrors `org.apache.pekko.testkit.TestKitSettings.dilated`, used by the
  /// test kit to scale every waiting time.
  pub fn dilated(&self, duration: Duration) -> Duration {
    duration.mul_f64(self.test_time_factor)
  }
}
