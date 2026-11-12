//! Unit tests for [`Timeout`](super::Timeout).

use core::time::Duration;

use super::Timeout;

#[test]
fn returns_the_wrapped_duration() {
  let timeout = Timeout::new(Duration::from_secs(5));

  assert_eq!(timeout.duration(), Duration::from_secs(5));
}

#[test]
fn compares_by_value() {
  let five_seconds = Timeout::new(Duration::from_secs(5));
  let also_five_seconds = Timeout::new(Duration::from_secs(5));
  let three_seconds = Timeout::new(Duration::from_secs(3));

  assert_eq!(five_seconds, also_five_seconds);
  assert_ne!(five_seconds, three_seconds);
}
