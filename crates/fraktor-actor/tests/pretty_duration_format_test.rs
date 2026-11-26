// Tests outside the original PrettyDurationSpec (design of #186,
// docs/design/issue-186-util-specs.md §2.1). They fix the `%g` formatting
// algorithm through the public `pretty_with`: Java's `Formatter` rounds the
// shortest decimal digits with HALF_UP, which differs from rounding the
// exact binary value. The expected values below were measured with JDK 26
// `String.format(Locale.ROOT, "%.<precision>g", value)`, except the
// out-of-range duration, whose expected value comes from the algorithm
// because Java cannot build that duration.

use core::{num::NonZeroUsize, time::Duration};

use fraktor_actor::kernel::pretty_duration::PrettyDurationInput;

fn finite_nanos(nanos: u128) -> PrettyDurationInput {
  PrettyDurationInput::Finite(Duration::from_nanos(nanos as u64))
}

fn precision(value: usize) -> NonZeroUsize {
  NonZeroUsize::new(value).expect("precision must be positive")
}

#[test]
fn pretty_duration_format_rounds_half_up_on_shortest_digits() {
  // 1_000_500 ns is 1.0005 ms: HALF_UP on the shortest digits gives 1.001,
  // rounding the exact binary value would give 1.000.
  assert_eq!(finite_nanos(1_000_500).pretty(), "1.001 ms");
  assert_eq!(finite_nanos(1_002_500).pretty(), "1.003 ms");
}

#[test]
fn pretty_duration_format_pads_after_shortest_digits_at_large_precision() {
  let input = PrettyDurationInput::Finite(Duration::from_nanos(1_100_000));
  assert_eq!(input.pretty_with(false, precision(20)), "1.1000000000000000000 ms");
}

#[test]
fn pretty_duration_format_keeps_unit_when_rounding_carries() {
  assert_eq!(finite_nanos(9_999_500).pretty(), "10.00 ms");
  assert_eq!(finite_nanos(59_999_999_999).pretty(), "60.00 s");
}

#[test]
fn pretty_duration_format_uses_java_style_exponent() {
  // 10_000 days: the rounded exponent selects the exponential form.
  assert_eq!(finite_nanos(864_000_000_000_000_000).pretty(), "1.000e+04 d");
  assert_eq!(finite_nanos(95).pretty_with(false, precision(1)), "1e+02 ns");
}

#[test]
fn pretty_duration_format_does_not_saturate_beyond_i64_nanos() {
  // About 1.157e7 days. The original cannot hold this range; the port keeps
  // the nanoseconds in u128, so the value is not silently saturated.
  let input = PrettyDurationInput::Finite(Duration::from_secs(1_000_000_000_000));
  assert_eq!(input.pretty(), "1.157e+07 d");
}

#[test]
fn pretty_duration_format_appends_nanos_when_asked() {
  let input = PrettyDurationInput::Finite(Duration::from_nanos(1_500));
  assert_eq!(input.pretty_with(true, precision(4)), "1.500 μs (1500 ns)");
}

#[test]
fn pretty_duration_format_zero() {
  assert_eq!(finite_nanos(0).pretty(), "0.000 ns");
  assert_eq!(finite_nanos(0).pretty_with(false, precision(1)), "0 ns");
}
