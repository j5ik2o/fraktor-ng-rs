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

fn narrow_sketch(capacity: i32, reset_multiplier: f64, counter_bits: i32) -> FrequencySketch<str> {
  FrequencySketch::new(capacity, FrequencySketchSettings { reset_multiplier, counter_bits, ..Default::default() })
    .expect("the settings are valid")
}

// The frequencies of `"0"` to `"39"`, each incremented `i % 3` times, and the size.
fn counts(mut sketch: FrequencySketch<str>) -> (Vec<i32>, i32) {
  for i in 0..40 {
    for _ in 0..i % 3 {
      sketch.increment(&i.to_string());
    }
  }
  ((0..40).map(|i| sketch.frequency(&i.to_string())).collect(), sketch.size())
}

// Widths of 4 and 16 counters are narrower than one word of 32 (2-bit) or 16
// (4-bit) counters. The original picks the counter in the word by the low bits
// of the position, not masked by the width, so positions that differ only above
// the width land in different counters of the same word. The expected values
// were read from Pekko 2.0.0's `FrequencySketch` on the JVM, with width
// multiplier 4 and depth 4.
#[test]
fn a_width_narrower_than_a_word_places_counters_as_the_original() {
  assert_eq!(
    counts(narrow_sketch(1, 1000.0, 2)),
    (
      [
        0, 1, 2, 0, 1, 2, 0, 2, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 3, 0, 1, 3, 0, 1, 2, 0, 1, 3, 0,
        1, 2, 0
      ]
      .to_vec(),
      38
    )
  );
  assert_eq!(
    counts(narrow_sketch(1, 1000.0, 4)),
    (
      [
        1, 2, 3, 0, 3, 3, 2, 2, 2, 0, 1, 2, 1, 2, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 1, 1, 4, 0, 2, 4, 0, 3, 3, 0, 1, 4, 0,
        2, 2, 0
      ]
      .to_vec(),
      39
    )
  );
  assert_eq!(trace(narrow_sketch(1, 10.0, 2)), [4, 8, 2, 6, 8]);
  assert_eq!(trace(narrow_sketch(1, 10.0, 4)), [5, 7, 9, 8, 9]);
  assert_eq!(trace(narrow_sketch(4, 10.0, 2)), [33, 19, 28, 14, 23]);
}

// The reset size keeps the low 32 bits of the product, as the original's
// `.toLong.toInt` does, so a negative multiplier can give a positive reset
// size: `-4294967295 * 1` truncates to 1, and every counted update resets. The
// expected values were read from Pekko 2.0.0 on the JVM.
#[test]
fn a_negative_reset_product_truncated_to_one_resets_every_update() {
  let mut sketch = narrow_sketch(1, -4_294_967_295.0, 4);
  for _ in 0..5 {
    sketch.increment("a");
    assert_eq!((sketch.size(), sketch.frequency("a")), (0, 0));
  }
  let mut sketch = narrow_sketch(1, -1.0, 4);
  for count in 1..=5 {
    sketch.increment("a");
    assert_eq!((sketch.size(), sketch.frequency("a")), (count, count));
  }
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

// A capacity above 2^30 rounds up to `i32::MIN`; the 64-bit width is negative
// and passes the original's overflow check, and its low 32 bits are zero.
#[test]
fn a_capacity_above_two_to_the_thirty_gives_a_width_of_zero() {
  let result = FrequencySketch::<str>::new(1_073_741_825, FrequencySketchSettings::default());
  assert_eq!(result.err(), Some(FrequencySketchError::WidthNotPositive { width: 0 }));
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
