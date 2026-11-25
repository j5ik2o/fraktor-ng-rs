use alloc::{string::ToString, vec::Vec};

use super::FrequencySketch;
use crate::kernel::frequency_sketch::{FrequencySketchError, FrequencySketchSettings};

fn sketch(reset_multiplier: f64, counter_bits: i32) -> FrequencySketch<str> {
  FrequencySketch::new(100, FrequencySketchSettings { reset_multiplier, counter_bits, ..Default::default() })
    .expect("the settings are valid")
}

// The size after every thousandth of 5000 increments of `((i * 7919) % 613).toString`.
fn trace(mut sketch: FrequencySketch<str>) -> Vec<i32> {
  let mut sizes = Vec::new();
  for i in 0..5000 {
    sketch.increment(&((i * 7919) % 613).to_string());
    if (i + 1) % 1000 == 0 {
      sizes.push(sketch.size());
    }
  }
  sizes
}

// The expected traces were read from Pekko 2.0.0's `FrequencySketch` on the JVM,
// with capacity 100, width multiplier 4 and depth 4. They pin the hashes, the
// counter placement and the reset arithmetic to the original's.
#[test]
fn the_size_follows_the_original_through_resets() {
  assert_eq!(trace(sketch(10.0, 4)), [433, 842, 636, 414, 802]);
  assert_eq!(trace(sketch(1.0, 2)), [16, 22, 34, 49, 52]);
  assert_eq!(trace(sketch(1000.0, 16)), [1000, 2000, 3000, 4000, 5000]);
}

#[test]
fn rejects_a_counter_size_that_is_not_supported() {
  let result = FrequencySketch::<str>::new(100, FrequencySketchSettings { counter_bits: 3, ..Default::default() });
  assert!(matches!(result, Err(FrequencySketchError::UnsupportedCounterBits { counter_bits: 3 })));
}

#[test]
fn rejects_a_width_that_is_not_a_power_of_two() {
  let result = FrequencySketch::<str>::new(100, FrequencySketchSettings { width_multiplier: 3, ..Default::default() });
  assert!(matches!(result, Err(FrequencySketchError::WidthNotPowerOfTwo { width: 384 })));
}

#[test]
fn reports_the_computed_width_when_it_overflows() {
  let result = FrequencySketch::<str>::new(536_870_913, FrequencySketchSettings::default());
  assert_eq!(result.err(), Some(FrequencySketchError::WidthOverflow { width: 4_294_967_296 }));
  let result =
    FrequencySketch::<str>::new(100, FrequencySketchSettings { width_multiplier: i32::MAX, ..Default::default() });
  assert_eq!(result.err(), Some(FrequencySketchError::WidthOverflow { width: 274_877_906_816 }));
}

#[test]
fn rejects_a_width_of_zero() {
  let result = FrequencySketch::<str>::new(100, FrequencySketchSettings { width_multiplier: 0, ..Default::default() });
  assert!(matches!(result, Err(FrequencySketchError::WidthNotPositive { width: 0 })));
}

#[test]
fn rejects_a_negative_depth() {
  let result = FrequencySketch::<str>::new(100, FrequencySketchSettings { depth: -1, ..Default::default() });
  assert!(matches!(result, Err(FrequencySketchError::NegativeDepth { depth: -1 })));
}

#[test]
fn a_sketch_without_rows_counts_nothing() {
  let mut sketch =
    FrequencySketch::<str>::new(100, FrequencySketchSettings { depth: 0, ..Default::default() }).expect("valid");
  sketch.increment("foo");
  assert_eq!(sketch.frequency("foo"), i32::MAX);
  assert_eq!(sketch.size(), 0);
}
