use alloc::{string::ToString, vec::Vec};

use super::FastFrequencySketch;
use crate::kernel::frequency_sketch::{FastFrequencySketchSettings, FrequencySketchError};

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
