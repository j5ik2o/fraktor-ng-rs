// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/FrequencySketchSpec.scala
//   actor-tests/src/test/scala/org/apache/pekko/util/ZipfianGenerator.scala (the helper)
//
// The original's fourteen cases under "FrequencySketch must" and eight under
// "FastFrequencySketch must" are kept as one test each, named after the case
// with a `frequency_sketch_` or `fast_frequency_sketch_` prefix, and with the
// original name as a comment. Every increment and check runs in the original
// order. Twenty of them are here. The two "detect width overflow that would
// have been missed by Int arithmetic" cases call `FrequencySketch.Bits`, which
// is `private[pekko]` in the original and crate-private in the port, so they
// are in the unit tests next to the sketches:
// src/kernel/frequency_sketch/frequency_sketch_value_test.rs and
// src/kernel/frequency_sketch/fast_frequency_sketch_test.rs.
//
// Rust has no default arguments. The original's
// `FrequencySketch[String](capacity, widthMultiplier = 4, resetMultiplier = 10,
// depth = 4, counterBits = 4)` is the port's
// `FrequencySketch::<str>::new(capacity, FrequencySketchSettings { .. })`,
// whose `Default` holds the same values, so a named argument in the original is
// the same field set over `..Default::default()` here. The implicit
// `Hasher[String]` is the default type parameter `StringHasher`.
// `FastFrequencySketch[String](capacity, widthMultiplier = 4, resetMultiplier =
// 10)` is `FastFrequencySketch::<str>::new(capacity, FastFrequencySketchSettings
// { .. })` in the same way.
//
// The original's `require` throws `IllegalArgumentException`. The port's
// constructors return `Err(FrequencySketchError::..)` instead, so
// `an[IllegalArgumentException] should be thrownBy` checks for the error.
//
// The original keeps the sketch's numbers in `Int` (size, frequency, capacity,
// width multiplier) and the counters in `Long`. The port keeps them in `i32`
// and in 64-bit words with the same wrapping arithmetic, so the case that shows
// that `Int` multiplication of the width overflows multiplies `i32` values with
// `wrapping_mul`, the JVM's `Int` multiplication.
//
// `ZipfianGenerator` is the original's test helper, not a node of its own. It is
// ported below with `scala.util.Random(seed)`, which is `java.util.Random`:
// the same 48-bit linear congruential generator and the same `nextDouble`.
//
// The test calls the production `FrequencySketch` and `FastFrequencySketch`.
// Until they exist the file does not compile: that compile error is the
// intended red for this node.

use std::collections::HashMap;

use fraktor_actor::kernel::frequency_sketch::{
  FastFrequencySketch, FastFrequencySketchSettings, FrequencySketch, FrequencySketchError, FrequencySketchSettings,
};

// `java.util.Random`, which `scala.util.Random(seed)` wraps.
struct JavaRandom {
  seed: u64,
}

impl JavaRandom {
  const ADDEND: u64 = 0xB;
  const MASK: u64 = (1 << 48) - 1;
  const MULTIPLIER: u64 = 0x5_DEEC_E66D;

  fn new(seed: i64) -> Self {
    Self { seed: (seed as u64 ^ Self::MULTIPLIER) & Self::MASK }
  }

  fn next(&mut self, bits: u32) -> i32 {
    self.seed = (self.seed.wrapping_mul(Self::MULTIPLIER).wrapping_add(Self::ADDEND)) & Self::MASK;
    (self.seed >> (48 - bits)) as i32
  }

  fn next_double(&mut self) -> f64 {
    let high = i64::from(self.next(26)) << 27;
    let low = i64::from(self.next(27));
    (high + low) as f64 * (1.0 / (1_u64 << 53) as f64)
  }
}

// The original's `ZipfianGenerator`, from "Quickly Generating Billion-Record
// Synthetic Databases", Jim Gray et al.
struct ZipfianGenerator {
  min: i32,
  n: i32,
  theta: f64,
  alpha: f64,
  zeta_n: f64,
  eta: f64,
  random: JavaRandom,
}

impl ZipfianGenerator {
  const DEFAULT_SEED: i32 = 502_539_523;
  const DEFAULT_THETA: f64 = 0.99;

  // The original's `ZipfianGenerator(n)`: `min = 0`, `max = n - 1`, and the
  // default theta and seed.
  fn new(n: i32) -> Self {
    Self::with(0, n - 1, Self::DEFAULT_THETA, Self::DEFAULT_SEED)
  }

  fn with(min: i32, max: i32, theta: f64, seed: i32) -> Self {
    let n = max - min + 1;
    let alpha = 1.0 / (1.0 - theta);
    let zeta2 = Self::zeta(2, theta);
    let zeta_n = Self::zeta(n, theta);
    let eta = (1.0 - (2.0 / f64::from(n)).powf(1.0 - theta)) / (1.0 - zeta2 / zeta_n);
    Self { min, n, theta, alpha, zeta_n, eta, random: JavaRandom::new(i64::from(seed)) }
  }

  fn zeta(n: i32, theta: f64) -> f64 {
    let mut sum = 0.0;
    for i in 1..=n {
      sum += 1.0 / f64::from(i).powf(theta);
    }
    sum
  }

  fn next(&mut self) -> i32 {
    let u = self.random.next_double();
    let uz = u * self.zeta_n;
    if uz < 1.0 {
      self.min
    } else if uz < 1.0 + 0.5_f64.powf(self.theta) {
      self.min + 1
    } else {
      self.min + (f64::from(self.n) * (self.eta * u - self.eta + 1.0).powf(self.alpha)) as i32
    }
  }
}

fn sketch(settings: FrequencySketchSettings) -> FrequencySketch<str> {
  FrequencySketch::new(100, settings).expect("the settings are valid")
}

fn fast_sketch(settings: FastFrequencySketchSettings) -> FastFrequencySketch<str> {
  FastFrequencySketch::new(100, settings).expect("the settings are valid")
}

// The original's `frequencies1.zip(frequencies2).foldLeft(0) { ... f2 == f1 / 2 ... }`.
fn halved(before: &[i32], after: &[i32]) -> i32 {
  before.iter().zip(after).fold(0, |correct, (f1, f2)| if *f2 == f1 / 2 { correct + 1 } else { correct })
}

// The original's zipfian case, which is the same for both sketches.
fn zipfian_accuracy(mut increment: impl FnMut(&str), frequency: impl Fn(&str) -> i32) -> f64 {
  let number_of_ids = 1000;
  let most_popular = 100;
  let mut zipfian = ZipfianGenerator::new(number_of_ids); // zipfian distribution, lower numbers are more popular
  let mut actual_frequencies: HashMap<i32, i32> = HashMap::new();
  for _ in 1..=100 * number_of_ids {
    let id = zipfian.next();
    increment(&id.to_string());
    *actual_frequencies.entry(id).or_insert(0) += 1;
  }
  // compare the most popular item frequencies with every other frequency, using order of actual frequency counts
  let mut sorted_actual_frequencies: Vec<(i32, i32)> = actual_frequencies.into_iter().collect();
  // The original sorts a Scala `mutable.Map` by count alone, which is stable over
  // the map's iteration order. The order among equal counts is not part of the
  // case; sorting by id after the count fixes it here.
  sorted_actual_frequencies.sort_by(|(id_a, count_a), (id_b, count_b)| count_b.cmp(count_a).then(id_a.cmp(id_b)));
  let mut comparisons = 0;
  let mut correct = 0;
  for i in 0..most_popular {
    let (id, _) = sorted_actual_frequencies[i];
    let frequency_of_id = frequency(&id.to_string());
    for (other_id, _) in &sorted_actual_frequencies[i + 1..] {
      let other_frequency = frequency(&other_id.to_string());
      if frequency_of_id >= other_frequency {
        correct += 1;
      }
      comparisons += 1;
    }
  }
  f64::from(correct) / f64::from(comparisons)
}

// --- "FrequencySketch must" ---

// "increment counters"
#[test]
fn frequency_sketch_increment_counters() {
  let mut sketch = sketch(FrequencySketchSettings::default());
  sketch.increment("foo");
  sketch.increment("bar");
  sketch.increment("bar");
  sketch.increment("baz");
  sketch.increment("baz");
  sketch.increment("baz");
  assert_eq!(sketch.frequency("foo"), 1);
  assert_eq!(sketch.frequency("bar"), 2);
  assert_eq!(sketch.frequency("baz"), 3);
  assert_eq!(sketch.size(), 6); // total number of increments
}

// "increment counters to max value"
#[test]
fn frequency_sketch_increment_counters_to_max_value() {
  // default of 4 bit counters, max value = 15
  let mut sketch = sketch(FrequencySketchSettings::default());
  for _ in 1..=20 {
    sketch.increment("foo");
  }
  assert_eq!(sketch.frequency("foo"), 15);
  assert_eq!(sketch.size(), 15); // total number of increments
}

// "reset counters when reset size is reached"
#[test]
fn frequency_sketch_reset_counters_when_reset_size_is_reached() {
  let mut sketch = sketch(FrequencySketchSettings { reset_multiplier: 10.0, ..Default::default() });

  // increment counters until the reset size
  for i in 1..=500 {
    sketch.increment(&i.to_string());
  }
  for i in 1..=499 {
    sketch.increment(&i.to_string());
  }
  assert_eq!(sketch.size(), 999);

  let frequencies1: Vec<i32> = (1..=499).map(|i| sketch.frequency(&i.to_string())).collect();

  // the 1000th increment will trigger a reset operation (halving all counters)
  sketch.increment("500");
  assert_eq!(sketch.size(), 500); // all counters (including hash collisions) will be even, so a perfect reset

  // frequencies should be halved now (ignore value 500, the reset trigger)
  let frequencies2: Vec<i32> = (1..=499).map(|i| sketch.frequency(&i.to_string())).collect();
  // note: it's possible that the value that triggers the reset has a hash collision and this ends up
  // bumping the minimum value for another counter, so that the expected halved frequency is off-by-one
  // this could happen to up to four other counters in the worst case, and is only an issue for testing
  assert!(halved(&frequencies1, &frequencies2) >= 499 - 4);

  // increment more values, creating odd counts and more hash collisions
  for i in 501..=999 {
    sketch.increment(&i.to_string());
  }
  assert_eq!(sketch.size(), 999);

  let frequencies3: Vec<i32> = (1..=999).map(|i| sketch.frequency(&i.to_string())).collect();

  // the 1000th increment will trigger a reset operation (halving counters)
  sketch.increment("1000");
  assert!(sketch.size() > 300 && sketch.size() < 500); // some counters will be odd numbers, rounded down when halved

  // frequencies should be halved now (ignore value 1000, the reset trigger)
  let frequencies4: Vec<i32> = (1..=999).map(|i| sketch.frequency(&i.to_string())).collect();
  assert!(halved(&frequencies3, &frequencies4) >= 999 - 4);
}

// "compare frequencies for more popular items with reasonable accuracy"
#[test]
fn frequency_sketch_compare_frequencies_for_more_popular_items_with_reasonable_accuracy() {
  let mut sketch = sketch(FrequencySketchSettings { reset_multiplier: 10.0, ..Default::default() });
  for i in 1000..=10000 {
    sketch.increment(&i.to_string()); // add some noise to the sketch
  }
  for i in 1..=5 {
    for _ in 1..=(6 - i) * 2 {
      sketch.increment(&i.to_string()); // 1-5 are most popular, in order
    }
  }
  for i in 1..=5 {
    assert!(sketch.frequency(&i.to_string()) >= sketch.frequency(&(i + 1).to_string()));
  }
  for i in 6..=10 {
    assert!(sketch.frequency("5") >= sketch.frequency(&i.to_string()));
  }
}

// "compare frequencies for random zipfian distribution with reasonable accuracy"
#[test]
fn frequency_sketch_compare_frequencies_for_random_zipfian_distribution_with_reasonable_accuracy() {
  let sketch = core::cell::RefCell::new(sketch(FrequencySketchSettings::default()));
  let accuracy =
    zipfian_accuracy(|value| sketch.borrow_mut().increment(value), |value| sketch.borrow().frequency(value));
  assert!(accuracy > 0.95, "accuracy {accuracy}"); // note: depends on the hash collisions, and random distribution
}

// "allow counter size to be configured as 2 bits"
#[test]
fn frequency_sketch_allow_counter_size_to_be_configured_as_2_bits() {
  let mut sketch = sketch(FrequencySketchSettings { counter_bits: 2, reset_multiplier: 1.0, ..Default::default() });

  // check increments
  sketch.increment("foo");
  sketch.increment("bar");
  sketch.increment("bar");
  sketch.increment("baz");
  sketch.increment("baz");
  sketch.increment("baz");
  assert_eq!(sketch.frequency("foo"), 1);
  assert_eq!(sketch.frequency("bar"), 2);
  assert_eq!(sketch.frequency("baz"), 3);
  assert_eq!(sketch.size(), 6); // total increments

  // check max value
  for _ in 1..=10 {
    sketch.increment("foo");
  }
  assert_eq!(sketch.frequency("foo"), 3); // max value
  assert_eq!(sketch.size(), 6 + 2); // total increments

  // check reset
  for i in 1..=(99 - 8) {
    sketch.increment(&i.to_string()); // up to reset size
  }
  assert_eq!(sketch.size(), 99);
  sketch.increment("qux"); // trigger reset
  assert!(sketch.size() <= 100 / 2);
}

// "allow counter size to be configured as 8 bits"
#[test]
fn frequency_sketch_allow_counter_size_to_be_configured_as_8_bits() {
  let mut sketch = sketch(FrequencySketchSettings { counter_bits: 8, ..Default::default() });

  // check increments
  sketch.increment("foo");
  sketch.increment("bar");
  sketch.increment("bar");
  sketch.increment("baz");
  sketch.increment("baz");
  sketch.increment("baz");
  assert_eq!(sketch.frequency("foo"), 1);
  assert_eq!(sketch.frequency("bar"), 2);
  assert_eq!(sketch.frequency("baz"), 3);
  assert_eq!(sketch.size(), 6); // total increments

  // check max value
  for _ in 1..=1000 {
    sketch.increment("foo");
  }
  assert_eq!(sketch.frequency("foo"), 255); // max value
  assert_eq!(sketch.size(), 6 + 254); // total increments

  // check reset
  for i in 1..=(999 - 260) {
    sketch.increment(&i.to_string()); // up to reset size
  }
  assert_eq!(sketch.size(), 999);
  sketch.increment("qux"); // trigger reset
  assert!(sketch.size() <= 1000 / 2);
}

// "allow counter size to be configured as 16 bits"
#[test]
fn frequency_sketch_allow_counter_size_to_be_configured_as_16_bits() {
  let mut sketch = sketch(FrequencySketchSettings { counter_bits: 16, reset_multiplier: 1000.0, ..Default::default() });

  // check increments
  sketch.increment("foo");
  sketch.increment("bar");
  sketch.increment("bar");
  sketch.increment("baz");
  sketch.increment("baz");
  sketch.increment("baz");
  assert_eq!(sketch.frequency("foo"), 1);
  assert_eq!(sketch.frequency("bar"), 2);
  assert_eq!(sketch.frequency("baz"), 3);
  assert_eq!(sketch.size(), 6); // total increments

  // check max value
  for _ in 1..=100_000 {
    sketch.increment("foo");
  }
  assert_eq!(sketch.frequency("foo"), 65535); // max value
  assert_eq!(sketch.size(), 6 + 65534); // total increments

  // check reset
  for i in 1..=(99_999 - 65_540) {
    sketch.increment(&i.to_string()); // up to reset size
  }
  assert_eq!(sketch.size(), 99_999);
  sketch.increment("qux"); // trigger reset
  assert!(sketch.size() <= 100_000 / 2);
}

// "allow counter size to be configured as 32 bits"
#[test]
fn frequency_sketch_allow_counter_size_to_be_configured_as_32_bits() {
  let mut sketch = sketch(FrequencySketchSettings { counter_bits: 32, ..Default::default() });

  // check increments
  sketch.increment("foo");
  sketch.increment("bar");
  sketch.increment("bar");
  sketch.increment("baz");
  sketch.increment("baz");
  sketch.increment("baz");
  assert_eq!(sketch.frequency("foo"), 1);
  assert_eq!(sketch.frequency("bar"), 2);
  assert_eq!(sketch.frequency("baz"), 3);
  assert_eq!(sketch.size(), 6); // total increments

  // max value is getting big to check, assume it's working

  // check reset (at 1000)
  for i in 1..=(999 - 6) {
    sketch.increment(&i.to_string()); // up to reset size
  }
  assert_eq!(sketch.size(), 999);
  sketch.increment("qux"); // trigger reset
  assert!(sketch.size() <= 1000 / 2);
}

// "allow counter size to be configured as 64 bits"
#[test]
fn frequency_sketch_allow_counter_size_to_be_configured_as_64_bits() {
  let mut sketch = sketch(FrequencySketchSettings { counter_bits: 64, ..Default::default() });

  // check increments
  sketch.increment("foo");
  sketch.increment("bar");
  sketch.increment("bar");
  sketch.increment("baz");
  sketch.increment("baz");
  sketch.increment("baz");
  assert_eq!(sketch.frequency("foo"), 1);
  assert_eq!(sketch.frequency("bar"), 2);
  assert_eq!(sketch.frequency("baz"), 3);
  assert_eq!(sketch.size(), 6); // total increments

  // max value is getting big to check, assume it's working

  // check reset (at 1000)
  for i in 1..=(999 - 6) {
    sketch.increment(&i.to_string()); // up to reset size
  }
  assert_eq!(sketch.size(), 999);
  sketch.increment("qux"); // trigger reset
  assert!(sketch.size() <= 1000 / 2);
}

// "allow depth to be configured"
#[test]
fn frequency_sketch_allow_depth_to_be_configured() {
  // different depths uses different number of hash functions
  // force a frequency sketch with just a single counter and hash function, which will always collide
  let mut sketch = FrequencySketch::<str>::new(
    1,
    FrequencySketchSettings { width_multiplier: 1, counter_bits: 64, depth: 1, ..Default::default() },
  )
  .expect("the settings are valid");
  sketch.increment("foo");
  sketch.increment("bar");
  assert_eq!(sketch.frequency("foo"), 2);
  assert_eq!(sketch.frequency("bar"), 2);
}

// "reject capacity that would cause width to overflow Int"
#[test]
fn frequency_sketch_reject_capacity_that_would_cause_width_to_overflow_int() {
  // capacity > 536,870,912 with default widthMultiplier=4: ceilingPowerOfTwo gives 2^30=1073741824, times 4 overflows
  let result = FrequencySketch::<str>::new(536_870_913, FrequencySketchSettings::default());
  assert!(matches!(result, Err(FrequencySketchError::WidthOverflow { .. })));
}

// "reject capacity that would cause width to overflow Int with custom widthMultiplier"
#[test]
fn frequency_sketch_reject_capacity_that_would_cause_width_to_overflow_int_with_custom_width_multiplier() {
  let result =
    FrequencySketch::<str>::new(100, FrequencySketchSettings { width_multiplier: i32::MAX, ..Default::default() });
  assert!(matches!(result, Err(FrequencySketchError::WidthOverflow { .. })));
}

// --- "FastFrequencySketch must" ---

// "increment counters"
#[test]
fn fast_frequency_sketch_increment_counters() {
  let mut sketch = fast_sketch(FastFrequencySketchSettings::default());
  sketch.increment("foo");
  sketch.increment("bar");
  sketch.increment("bar");
  sketch.increment("baz");
  sketch.increment("baz");
  sketch.increment("baz");
  assert_eq!(sketch.frequency("foo"), 1);
  assert_eq!(sketch.frequency("bar"), 2);
  assert_eq!(sketch.frequency("baz"), 3);
  assert_eq!(sketch.size(), 6); // total number of increments
}

// "increment counters to max value"
#[test]
fn fast_frequency_sketch_increment_counters_to_max_value() {
  let mut sketch = fast_sketch(FastFrequencySketchSettings::default());
  for _ in 1..=20 {
    sketch.increment("foo");
  }
  assert_eq!(sketch.frequency("foo"), 15);
  assert_eq!(sketch.size(), 15); // total number of increments
}

// "reset counters when reset size is reached"
#[test]
fn fast_frequency_sketch_reset_counters_when_reset_size_is_reached() {
  let mut sketch = fast_sketch(FastFrequencySketchSettings { reset_multiplier: 10.0, ..Default::default() });

  // increment counters until the reset size
  for i in 1..=500 {
    sketch.increment(&i.to_string());
  }
  for i in 1..=499 {
    sketch.increment(&i.to_string());
  }
  assert_eq!(sketch.size(), 999);

  let frequencies1: Vec<i32> = (1..=499).map(|i| sketch.frequency(&i.to_string())).collect();

  // the 1000th increment will trigger a reset operation (halving all counters)
  sketch.increment("500");
  assert_eq!(sketch.size(), 500); // all counters (including hash collisions) will be even, so a perfect reset

  // frequencies should be halved now (ignore value 500, the reset trigger)
  let frequencies2: Vec<i32> = (1..=499).map(|i| sketch.frequency(&i.to_string())).collect();
  // note: it's possible that the value that triggers the reset has a hash collision and this ends up
  // bumping the minimum value for another counter, so that the expected halved frequency is off-by-one
  // this could happen to up to four other counters in the worst case, and is only an issue for testing
  assert!(halved(&frequencies1, &frequencies2) >= 499 - 4);

  // increment more values, creating odd counts and more hash collisions
  for i in 501..=999 {
    sketch.increment(&i.to_string());
  }
  assert_eq!(sketch.size(), 999);

  let frequencies3: Vec<i32> = (1..=999).map(|i| sketch.frequency(&i.to_string())).collect();

  // the 1000th increment will trigger a reset operation (halving counters)
  sketch.increment("1000");
  assert!(sketch.size() > 300 && sketch.size() < 500); // some counters will be odd numbers, rounded down when halved

  // frequencies should be halved now (ignore value 1000, the reset trigger)
  let frequencies4: Vec<i32> = (1..=999).map(|i| sketch.frequency(&i.to_string())).collect();
  assert!(halved(&frequencies3, &frequencies4) >= 999 - 4);
}

// "compare frequencies for more popular items with reasonable accuracy"
#[test]
fn fast_frequency_sketch_compare_frequencies_for_more_popular_items_with_reasonable_accuracy() {
  let mut sketch = fast_sketch(FastFrequencySketchSettings { reset_multiplier: 10.0, ..Default::default() });
  for i in 1000..=10000 {
    sketch.increment(&i.to_string()); // add some noise to the sketch
  }
  for i in 1..=5 {
    for _ in 1..=(6 - i) * 2 {
      sketch.increment(&i.to_string()); // 1-5 are most popular, in order
    }
  }
  for i in 1..=5 {
    assert!(sketch.frequency(&i.to_string()) >= sketch.frequency(&(i + 1).to_string()));
  }
  for i in 6..=10 {
    assert!(sketch.frequency("5") >= sketch.frequency(&i.to_string()));
  }
}

// "compare frequencies for random zipfian distribution with reasonable accuracy"
#[test]
fn fast_frequency_sketch_compare_frequencies_for_random_zipfian_distribution_with_reasonable_accuracy() {
  let sketch = core::cell::RefCell::new(fast_sketch(FastFrequencySketchSettings::default()));
  let accuracy =
    zipfian_accuracy(|value| sketch.borrow_mut().increment(value), |value| sketch.borrow().frequency(value));
  assert!(accuracy > 0.95, "accuracy {accuracy}"); // note: depends on the hash collisions, and random distribution
}

// "reject capacity that would cause width to overflow Int"
#[test]
fn fast_frequency_sketch_reject_capacity_that_would_cause_width_to_overflow_int() {
  // capacity > 536,870,912 with default widthMultiplier=4: ceilingPowerOfTwo gives 2^30=1073741824, times 4 overflows
  let result = FastFrequencySketch::<str>::new(536_870_913, FastFrequencySketchSettings::default());
  assert!(matches!(result, Err(FrequencySketchError::WidthOverflow { .. })));
}

// "reject capacity that would cause width to overflow Int with custom widthMultiplier"
#[test]
fn fast_frequency_sketch_reject_capacity_that_would_cause_width_to_overflow_int_with_custom_width_multiplier() {
  let result = FastFrequencySketch::<str>::new(
    100,
    FastFrequencySketchSettings { width_multiplier: i32::MAX, ..Default::default() },
  );
  assert!(matches!(result, Err(FrequencySketchError::WidthOverflow { .. })));
}
