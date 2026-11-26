use alloc::{string::ToString, vec::Vec};

use super::FastFrequencySketch;
use crate::kernel::frequency_sketch::{bits::Bits, FastFrequencySketchSettings, FrequencySketchError};

// The size after every thousandth of 5000 increments of `((i * 7919) % 613).toString`.
fn trace(reset_multiplier: f64) -> Vec<i32> {
  let mut sketch =
    FastFrequencySketch::<str>::new(100, FastFrequencySketchSettings { reset_multiplier, ..Default::default() })
      .expect("the settings are valid");
  let mut sizes = Vec::new();
  for i in 0..5000 {
    sketch.increment(&((i * 7919) % 613).to_string());
    if (i + 1) % 1000 == 0 {
      sizes.push(sketch.size());
    }
  }
  sizes
}

// The expected traces were read from Pekko 2.0.0's `FastFrequencySketch` on the
// JVM, with capacity 100 and width multiplier 4. They pin the rehash, the seeds
// and the reset arithmetic to the original's.
#[test]
fn the_size_follows_the_original_through_resets() {
  assert_eq!(trace(10.0), [428, 841, 631, 408, 795]);
  assert_eq!(trace(1.0), [24, 49, 78, 20, 44]);
}

#[test]
fn rejects_a_width_that_is_not_a_power_of_two() {
  let result =
    FastFrequencySketch::<str>::new(100, FastFrequencySketchSettings { width_multiplier: 3, ..Default::default() });
  assert!(matches!(result, Err(FrequencySketchError::WidthNotPowerOfTwo { width: 384 })));
}

// See the same case of the configurable sketch.
#[test]
fn a_capacity_above_two_to_the_thirty_gives_a_width_of_zero() {
  let result = FastFrequencySketch::<str>::new(1_073_741_825, FastFrequencySketchSettings::default());
  assert_eq!(result.err(), Some(FrequencySketchError::WidthNotPositive { width: 0 }));
}

#[test]
fn rejects_a_width_of_zero() {
  let result =
    FastFrequencySketch::<str>::new(100, FastFrequencySketchSettings { width_multiplier: 0, ..Default::default() });
  assert!(matches!(result, Err(FrequencySketchError::WidthNotPositive { width: 0 })));
}

// A capacity of zero or below is accepted, as the original accepts it. The
// reset is disabled unless the reset multiplier is negative too, and
// `i32::MIN` alone gives `WidthNotPositive`.
#[test]
fn a_capacity_of_zero_or_below_is_accepted_as_the_original() {
  let mut zero =
    FastFrequencySketch::<str>::new(0, FastFrequencySketchSettings::default()).expect("the original accepts 0");
  for _ in 0..5 {
    zero.increment("foo");
  }
  assert_eq!(zero.frequency("foo"), 5);
  assert_eq!(zero.size(), 5);

  let settings = FastFrequencySketchSettings { reset_multiplier: -2.0, ..Default::default() };
  let mut negative = FastFrequencySketch::<str>::new(-1, settings).expect("the original accepts -1");
  negative.increment("foo");
  negative.increment("foo");
  assert_eq!(negative.frequency("foo"), 1);

  // -1 * 4294967295.0 is negative, but its low 32 bits are 1: the reset runs
  // on every update.
  let settings = FastFrequencySketchSettings { reset_multiplier: 4_294_967_295.0, ..Default::default() };
  let mut truncated = FastFrequencySketch::<str>::new(-1, settings).expect("the original accepts -1");
  for _ in 0..4 {
    truncated.increment("foo");
    assert_eq!((truncated.frequency("foo"), truncated.size()), (0, 0));
  }

  let result = FastFrequencySketch::<str>::new(i32::MIN, FastFrequencySketchSettings::default());
  assert!(matches!(result, Err(FrequencySketchError::WidthNotPositive { width: 0 })));
}

// A case of Pekko v2.0.0's `FrequencySketchSpec` (SHA
// 9fbd2283b1cf701c90a1223bf621942c191db61f), under "FastFrequencySketch must", and one of
// the ledger's 22. Twenty are in tests/frequency_sketch_spec.rs; this one and
// its twin in frequency_sketch_value_test.rs are here because they call
// `FrequencySketch.Bits`, which is `private[pekko]` in the original and
// crate-private in the port.
// "detect width overflow that would have been missed by Int arithmetic"
#[test]
fn fast_frequency_sketch_detect_width_overflow_that_would_have_been_missed_by_int_arithmetic() {
  // before the fix, widthMultiplier * ceilingPowerOfTwo(capacity) was computed in Int and could silently overflow
  let capacity: i32 = 536_870_913;
  let width_multiplier: i32 = 4;
  let width_int = width_multiplier.wrapping_mul(Bits::ceiling_power_of_two(capacity));
  assert!(width_int <= 0); // negative or zero due to Int overflow (old broken behavior)
  let width_long = i64::from(width_multiplier) * i64::from(Bits::ceiling_power_of_two(capacity));
  assert!(width_long > i64::from(i32::MAX));
}
