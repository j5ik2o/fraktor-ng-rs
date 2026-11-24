//! Unit tests for [`Ttl`](super::Ttl).

use core::time::Duration;

use super::{NonPositiveTtlError, Ttl};

// The original's `Ttl.fromPositive` rejects zero, so a zero `Duration` is the
// only value it can reject and must not pass through.
#[test]
fn a_zero_duration_is_rejected() {
  assert_eq!(Ttl::from_positive(Duration::ZERO), Err(NonPositiveTtlError));
}

// One nanosecond is the smallest positive `Duration`, so accepting it and
// returning the same value is the accepted side of the zero boundary.
#[test]
fn one_nanosecond_is_accepted_unchanged() {
  let ttl = Ttl::from_positive(Duration::from_nanos(1)).expect("a positive duration");
  assert_eq!(ttl.value(), Duration::from_nanos(1));
}

// A larger duration is kept as given, not rounded or clamped.
#[test]
fn a_larger_duration_is_accepted_unchanged() {
  let ttl = Ttl::from_positive(Duration::from_secs(10)).expect("a positive duration");
  assert_eq!(ttl.value(), Duration::from_secs(10));
}

#[test]
fn the_display_text_names_the_failure() {
  assert!(!NonPositiveTtlError.to_string().is_empty());
}
