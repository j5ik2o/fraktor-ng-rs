//! Unit tests for [`TestKitSettings`](super::TestKitSettings).

use core::time::Duration;

use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::timeout::Timeout;

use super::TestKitSettings;

fn settings() -> TestKitSettings {
  TestKitSettings::from_system(&ActorSystem::new("TestKitSettingsSpec", Arc::new(ChannelBoundary::new())))
}

#[test]
fn default_timeout_is_five_seconds() {
  assert_eq!(settings().default_timeout(), Timeout::new(Duration::from_secs(5)));
}

#[test]
fn single_expect_default_timeout_is_three_seconds() {
  assert_eq!(settings().single_expect_default_timeout(), Duration::from_secs(3));
}

#[test]
fn expect_no_message_default_timeout_is_hundred_milliseconds() {
  assert_eq!(settings().expect_no_message_default_timeout(), Duration::from_millis(100));
}

#[test]
fn test_event_filter_leeway_is_three_seconds() {
  assert_eq!(settings().test_event_filter_leeway(), Duration::from_secs(3));
}

#[test]
fn test_time_factor_is_one() {
  assert_eq!(settings().test_time_factor(), 1.0);
}

#[test]
fn default_timeout_differs_from_the_other_timeouts() {
  let settings = settings();

  assert_ne!(settings.default_timeout().duration(), settings.single_expect_default_timeout());
  assert_ne!(settings.default_timeout().duration(), settings.expect_no_message_default_timeout());
  assert_ne!(settings.default_timeout().duration(), settings.test_event_filter_leeway());
}

#[test]
fn dilated_scales_a_duration_by_the_time_factor() {
  let settings = settings();

  assert_eq!(settings.dilated(Duration::from_secs(3)), Duration::from_secs(3));
}
