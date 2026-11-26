use super::Bits;

// The expected values were read from Pekko 2.0.0's `FrequencySketch.Bits` on the JVM.
#[test]
fn ceiling_power_of_two_matches_the_original() {
  assert_eq!(Bits::ceiling_power_of_two(536_870_913), 1_073_741_824);
  assert_eq!(Bits::ceiling_power_of_two(0), 1);
  assert_eq!(Bits::ceiling_power_of_two(1), 1);
  assert_eq!(Bits::ceiling_power_of_two(100), 128);
  assert_eq!(Bits::ceiling_power_of_two(-5), 1);
  assert_eq!(Bits::ceiling_power_of_two(1_073_741_825), i32::MIN);
}

#[test]
fn power_of_two_exponent_matches_the_original() {
  assert_eq!(Bits::power_of_two_exponent(64), 6);
  assert_eq!(Bits::power_of_two_exponent(1), 0);
}

#[test]
fn is_power_of_two_matches_the_original() {
  assert!(Bits::is_power_of_two(0));
  assert!(Bits::is_power_of_two(64));
  assert!(!Bits::is_power_of_two(96));
  assert!(Bits::is_power_of_two(i32::MIN));
}

// P6 of the design (docs/design/issue-177-frequency-sketch.md §3): `Bits`
// agrees with the definitions on any `i32`. The inputs come from a fixed-seed
// SplitMix64, the same generator, seed and draws as the other properties in
// tests/frequency_sketch_property_test.rs.
struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  const fn new(seed: u64) -> Self {
    Self { state: seed }
  }

  const fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  const fn below(&mut self, bound: usize) -> usize {
    (self.next_u64() % bound as u64) as usize
  }

  const fn i32(&mut self) -> i32 {
    self.next_u64() as i32
  }
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
