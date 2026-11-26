//! Power-of-two helpers on 32-bit integers.

#[cfg(test)]
#[path = "bits_test.rs"]
mod tests;

/// Power-of-two arithmetic on `i32`, with the wrapping and shift semantics of
/// 32-bit two's complement integers whose shift counts are taken modulo 32.
///
/// Mirrors `FrequencySketch.Bits` in `org.apache.pekko.util`.
pub struct Bits;

impl Bits {
  /// Returns the smallest power of two that is at least `value`.
  ///
  /// Values below one give one. A value above `2^30` gives `i32::MIN`, the
  /// bit pattern of `2^31`.
  #[must_use]
  pub const fn ceiling_power_of_two(value: i32) -> i32 {
    let leading_zeros = value.wrapping_sub(1).leading_zeros();
    1_i32.wrapping_shl(0_u32.wrapping_sub(leading_zeros))
  }

  /// Returns whether `value` has at most one bit set, so zero and `i32::MIN`
  /// count as powers of two.
  #[must_use]
  pub const fn is_power_of_two(value: i32) -> bool {
    value & value.wrapping_sub(1) == 0
  }

  /// Returns the exponent of the smallest power of two that is at least
  /// `value`.
  #[must_use]
  pub const fn power_of_two_exponent(value: i32) -> i32 {
    32 - value.wrapping_sub(1).leading_zeros() as i32
  }
}
