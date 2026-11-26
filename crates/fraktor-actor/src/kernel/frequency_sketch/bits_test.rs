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
