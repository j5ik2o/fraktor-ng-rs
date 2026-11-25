// Property-based tests of the frequency sketches (the design of #177,
// docs/goals/issue-177-frequency-sketch-design.md §3, P1 to P7).
//
// These are not ports of Pekko cases and are not counted in the ledger. The
// inputs come from a fixed-seed SplitMix64, so a failure repeats; every
// failure message names the seed. No case is excluded: depth zero, disabled
// resets, NaN multipliers, every counter size, the empty string, non-ASCII
// strings, surrogate pairs and long strings are all drawn.

use std::collections::BTreeMap;

use fraktor_actor::kernel::frequency_sketch::{
  Bits, FastFrequencySketch, FastFrequencySketchSettings, FrequencySketch, FrequencySketchError, FrequencySketchHasher,
  FrequencySketchSettings, StringHashCode, StringHasher,
};

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn new(seed: u64) -> Self {
    Self { state: seed }
  }

  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: usize) -> usize {
    (self.next_u64() % bound as u64) as usize
  }

  fn pick<T: Copy>(&mut self, items: &[T]) -> T {
    items[self.below(items.len())]
  }

  fn i32(&mut self) -> i32 {
    self.next_u64() as i32
  }
}

// Pieces of strings: ASCII, two- and three-byte UTF-8, and characters outside
// the BMP, which are surrogate pairs in UTF-16.
const PIECES: [&str; 12] = ["a", "b", "z", "0", "9", "foo", "é", "ß", "日", "本", "😀", "𝄞"];

fn random_string(random: &mut SplitMix64) -> String {
  match random.below(10) {
    | 0 => String::new(),
    | 1 => (0..random.below(400) + 200).map(|_| random.pick(&PIECES)).collect(),
    | _ => (0..random.below(6) + 1).map(|_| random.pick(&PIECES)).collect(),
  }
}

fn pool(random: &mut SplitMix64) -> Vec<String> {
  let mut values: Vec<String> = (0..random.below(40) + 1).map(|_| random_string(random)).collect();
  values.sort();
  values.dedup();
  values
}

// --- reference definitions, written from the original's sources ------------------

// `scala.util.hashing.MurmurHash3.stringHash`, over the UTF-16 code units by
// index, as the Scala source loops.
fn reference_string_hash(value: &str, seed: i32) -> i32 {
  fn mix_last(hash: i32, data: i32) -> i32 {
    let mut k = data.wrapping_mul(0xCC9E_2D51_u32 as i32);
    k = k.rotate_left(15);
    k = k.wrapping_mul(0x1B87_3593);
    hash ^ k
  }
  fn mix(hash: i32, data: i32) -> i32 {
    let h = mix_last(hash, data).rotate_left(13);
    h.wrapping_mul(5).wrapping_add(0xE654_6B64_u32 as i32)
  }
  let units: Vec<u16> = value.encode_utf16().collect();
  let mut h = seed;
  let mut i = 0;
  while i + 1 < units.len() {
    let data = (i32::from(units[i]) << 16).wrapping_add(i32::from(units[i + 1]));
    h = mix(h, data);
    i += 2;
  }
  if i < units.len() {
    h = mix_last(h, i32::from(units[i]));
  }
  h ^= units.len() as i32;
  h ^= ((h as u32) >> 16) as i32;
  h = h.wrapping_mul(0x85EB_CA6B_u32 as i32);
  h ^= ((h as u32) >> 13) as i32;
  h = h.wrapping_mul(0xC2B2_AE35_u32 as i32);
  h ^ ((h as u32) >> 16) as i32
}

// `java.lang.String.hashCode`, character by character.
fn reference_hash_code(value: &str) -> i32 {
  let mut hash: i32 = 0;
  let mut buffer = [0_u16; 2];
  for character in value.chars() {
    for unit in character.encode_utf16(&mut buffer) {
      hash = hash.wrapping_mul(31).wrapping_add(i32::from(*unit));
    }
  }
  hash
}

// The smallest power of two at least `value`, with the original's results
// outside 1 to 2^30. At `i32::MIN`, the original subtracts one first, which
// wraps to `i32::MAX` and shifts by 31, so the result is `i32::MIN` itself.
fn reference_ceiling_power_of_two(value: i32) -> i32 {
  if value == i32::MIN {
    i32::MIN
  } else if value <= 1 {
    1
  } else if value > 1 << 30 {
    i32::MIN
  } else {
    (value as u32).next_power_of_two() as i32
  }
}

// What `FrequencySketch.apply` and the constructor accept, in their order.
fn reference_settings_error(
  capacity: i32,
  width_multiplier: i32,
  counter_bits: Option<i32>,
  depth: Option<i32>,
) -> Option<FrequencySketchError> {
  let wide = i64::from(width_multiplier) * i64::from(reference_ceiling_power_of_two(capacity));
  if wide > i64::from(i32::MAX) {
    return Some(FrequencySketchError::WidthOverflow { width: wide });
  }
  let width = wide as i32;
  if width != 0 && width != i32::MIN && (width < 0 || (width as u32).count_ones() != 1) {
    return Some(FrequencySketchError::WidthNotPowerOfTwo { width });
  }
  if width <= 0 {
    return Some(FrequencySketchError::WidthNotPositive { width });
  }
  if let Some(bits) = counter_bits {
    if ![2, 4, 8, 16, 32, 64].contains(&bits) {
      return Some(FrequencySketchError::UnsupportedCounterBits { counter_bits: bits });
    }
  }
  if let Some(depth) = depth {
    if depth < 0 {
      return Some(FrequencySketchError::NegativeDepth { depth });
    }
  }
  None
}

fn reference_reset_size(capacity: i32, reset_multiplier: f64) -> i32 {
  i64::from(i32::MAX).min((reset_multiplier * f64::from(capacity)) as i64) as i32
}

// --- the sketch under test -------------------------------------------------------

enum Sketch {
  General(FrequencySketch<str>),
  Fast(FastFrequencySketch<str>),
}

impl Sketch {
  fn increment(&mut self, value: &str) {
    match self {
      | Self::General(sketch) => sketch.increment(value),
      | Self::Fast(sketch) => sketch.increment(value),
    }
  }

  fn frequency(&self, value: &str) -> i32 {
    match self {
      | Self::General(sketch) => sketch.frequency(value),
      | Self::Fast(sketch) => sketch.frequency(value),
    }
  }

  fn size(&self) -> i32 {
    match self {
      | Self::General(sketch) => sketch.size(),
      | Self::Fast(sketch) => sketch.size(),
    }
  }
}

struct Shape {
  depth: i32,
  // `None` for 32- and 64-bit counters, whose maximum does not fit an `i32`.
  max_count: Option<i32>,
  reset_size: i32,
}

fn random_sketch(random: &mut SplitMix64) -> (Sketch, Shape, String) {
  let capacity = if random.below(4) == 0 { random.pick(&[1, 2, 3]) } else { random.below(200) as i32 + 1 };
  let width_multiplier = random.pick(&[1, 2, 4, 8, 16]);
  let reset_multiplier = random.pick(&[0.0, -1.0, 0.5, 1.0, 2.0, 10.0, f64::NAN, 1e12]);
  let reset_size = reference_reset_size(capacity, reset_multiplier);
  if random.below(3) == 0 {
    let settings = FastFrequencySketchSettings { width_multiplier, reset_multiplier };
    let sketch = FastFrequencySketch::new(capacity, settings).expect("the settings are valid");
    let label = format!("fast capacity={capacity} {settings:?}");
    (Sketch::Fast(sketch), Shape { depth: 4, max_count: Some(15), reset_size }, label)
  } else {
    let counter_bits = random.pick(&[2, 4, 8, 16, 32, 64]);
    let depth = random.pick(&[0, 1, 2, 3, 4, 5, 8]);
    let settings = FrequencySketchSettings { width_multiplier, reset_multiplier, depth, counter_bits };
    let sketch = FrequencySketch::new(capacity, settings).expect("the settings are valid");
    let max_count = if counter_bits <= 16 { Some((1 << counter_bits) - 1) } else { None };
    let label = format!("capacity={capacity} {settings:?}");
    (Sketch::General(sketch), Shape { depth, max_count, reset_size }, label)
  }
}

// P1 to P4 on one random sketch and one random run of increments.
fn check_run(seed: u64) {
  let mut random = SplitMix64::new(seed);
  let (mut sketch, shape, label) = random_sketch(&mut random);
  let values = pool(&mut random);
  let steps = random.pick(&[50, 300, 2000]);
  let context = |step: usize, what: &str| format!("seed={seed} step={step} {label}: {what}");
  let mut true_counts: BTreeMap<&str, i32> = BTreeMap::new();
  let mut increments = 0;
  let mut reset_seen = false;
  for step in 0..steps {
    let value = values[random.below(values.len())].as_str();
    let before_size = sketch.size();
    let before: Vec<i32> = values.iter().map(|v| sketch.frequency(v)).collect();
    let trigger_saturated = Some(sketch.frequency(value)) == shape.max_count;
    sketch.increment(value);
    increments += 1;
    *true_counts.entry(value).or_insert(0) += 1;
    let after_size = sketch.size();

    // P1: the size is never negative, never above the increments, and stays
    // below an enabled reset size.
    assert!(after_size >= 0, "{}", context(step, "negative size"));
    assert!(after_size <= increments, "{}", context(step, "size above the increments"));
    if shape.reset_size >= 1 {
      assert!(after_size < shape.reset_size, "{}", context(step, "size at or above the reset size"));
    }
    if shape.depth == 0 {
      assert_eq!(after_size, 0, "{}", context(step, "a sketch without rows counted"));
    }

    let resets = shape.depth > 0 && shape.reset_size >= 1 && before_size + 1 == shape.reset_size && !trigger_saturated;
    for (index, other) in values.iter().enumerate() {
      let frequency = sketch.frequency(other);
      // P2: frequencies stay between zero and the counter maximum, and are
      // `i32::MAX` without rows. A 32- or 64-bit counter's maximum does not
      // fit an `i32`, so only the lower bound is checked; at most 2000 steps
      // cannot reach the 2^31 where the original's `toInt` truncation turns
      // over.
      if shape.depth == 0 {
        assert_eq!(frequency, i32::MAX, "{}", context(step, "frequency without rows"));
        continue;
      }
      assert!(frequency >= 0, "{}", context(step, "negative frequency"));
      if let Some(max_count) = shape.max_count {
        assert!(frequency <= max_count, "{}", context(step, "frequency above the counter maximum"));
      }
      if resets {
        // P4: the reset halves every frequency, give or take the trigger's own
        // increment: floor(f / 2) <= f' <= floor((f + 1) / 2).
        let f = before[index];
        assert!(
          f / 2 <= frequency && frequency <= (f + 1) / 2,
          "{}",
          context(step, &format!("frequency of {other:?} went from {f} to {frequency} at the reset"))
        );
      } else if !reset_seen {
        // P3: before the first reset, no value is underestimated beyond the
        // counter maximum.
        let truth = true_counts.get(other.as_str()).copied().unwrap_or(0);
        let floor = shape.max_count.map_or(truth, |max_count| truth.min(max_count));
        assert!(frequency >= floor, "{}", context(step, &format!("{other:?} estimated {frequency} below {floor}")));
      }
    }
    if resets {
      reset_seen = true;
      assert!(after_size <= shape.reset_size / 2, "{}", context(step, "size above half the reset size after a reset"));
    } else if shape.depth > 0 {
      let expected = if trigger_saturated { before_size } else { before_size + 1 };
      assert_eq!(after_size, expected, "{}", context(step, "an update was not counted once"));
    }
  }
}

#[test]
fn random_runs_keep_the_sketch_invariants() {
  for seed in 0..300 {
    check_run(seed);
  }
}

// P5: the hashers equal the original's definitions on any string.
#[test]
fn the_hashers_equal_the_original_definitions() {
  let mut random = SplitMix64::new(0x5EED);
  for case in 0..3000 {
    let value = random_string(&mut random);
    let seed = if case % 2 == 0 { StringHasher::DEFAULT_SEED } else { random.i32() };
    assert_eq!(StringHasher::new(seed).hash(value.as_str()), reference_string_hash(&value, seed), "{value:?} {seed}");
    assert_eq!(StringHashCode.hash(value.as_str()), reference_hash_code(&value), "{value:?}");
  }
  // A surrogate pair, as on the JVM: 0xD83D * 31 + 0xDE00.
  assert_eq!(StringHashCode.hash("😀"), 1_772_899);
}

// P6: `Bits` agrees with the definitions on any `i32`.
#[test]
fn bits_follow_their_definitions() {
  let mut random = SplitMix64::new(0xB175);
  let edges = [i32::MIN, i32::MIN + 1, -5, -1, 0, 1, 2, 3, 1 << 30, (1 << 30) + 1, i32::MAX];
  let values = edges.into_iter().chain((0..20_000).map(|_| random.i32() >> random.below(31)));
  for value in values {
    let ceiling = Bits::ceiling_power_of_two(value);
    assert_eq!(ceiling, reference_ceiling_power_of_two(value), "ceiling_power_of_two({value})");
    assert_eq!(Bits::is_power_of_two(value), value & value.wrapping_sub(1) == 0, "is_power_of_two({value})");
    if (1..=1 << 30).contains(&value) {
      assert_eq!(1_i64 << Bits::power_of_two_exponent(value), i64::from(ceiling), "power_of_two_exponent({value})");
    }
  }
}

// P7: the constructors accept exactly the settings the original accepts and
// can index, and report the first failure in the original's order.
#[test]
fn constructors_accept_exactly_the_valid_settings() {
  let mut random = SplitMix64::new(0xC0F1);
  let capacities = [i32::MIN, -1, 0, 1, 2, 100, 536_870_912, 536_870_913, 1 << 30, (1 << 30) + 1, i32::MAX];
  let multipliers = [i32::MIN, -4, -1, 0, 1, 3, 4, 8, i32::MAX];
  for case in 0..20_000 {
    let capacity = if case % 2 == 0 { random.pick(&capacities) } else { random.i32() >> random.below(31) };
    let width_multiplier = if case % 3 == 0 { random.pick(&multipliers) } else { random.i32() >> random.below(31) };
    let counter_bits = random.pick(&[-1, 0, 1, 2, 3, 4, 8, 16, 32, 63, 64, 128]);
    let depth = random.pick(&[i32::MIN, -1, 0, 1, 4, 8]);
    let settings = FrequencySketchSettings { width_multiplier, reset_multiplier: 10.0, depth, counter_bits };
    // Building a valid sketch allocates its counters, so only small widths
    // are built; the rest are checked through the error alone.
    let expected = reference_settings_error(capacity, width_multiplier, Some(counter_bits), Some(depth));
    let small = i64::from(width_multiplier) * i64::from(reference_ceiling_power_of_two(capacity)) <= 1 << 16;
    if expected.is_some() || small {
      let actual = FrequencySketch::<str>::new(capacity, settings).err();
      assert_eq!(actual, expected, "capacity={capacity} {settings:?}");
      let fast_settings = FastFrequencySketchSettings { width_multiplier, reset_multiplier: 10.0 };
      let fast = FastFrequencySketch::<str>::new(capacity, fast_settings).err();
      assert_eq!(fast, reference_settings_error(capacity, width_multiplier, None, None), "capacity={capacity}");
    }
  }
}
