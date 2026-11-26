//! The validated number of counters per sketch row.

use super::{bits::Bits, FrequencySketchError};

// The number of counters per row: a positive power of two that fits in an
// `i32`, so a hash masked with `width - 1` picks a counter.
#[derive(Clone, Copy)]
pub(super) struct SketchWidth {
  width: i32,
}

impl SketchWidth {
  // The width multiplier times the capacity rounded up to a power of two,
  // computed in 64 bits as the original's `apply` does.
  pub(super) fn of(capacity: i32, width_multiplier: i32) -> Result<Self, FrequencySketchError> {
    let wide = i64::from(width_multiplier) * i64::from(Bits::ceiling_power_of_two(capacity));
    if wide > i64::from(i32::MAX) {
      return Err(FrequencySketchError::WidthOverflow { width: wide });
    }
    // The original's `toInt`, which keeps the low 32 bits.
    let width = wide as i32;
    if !Bits::is_power_of_two(width) {
      return Err(FrequencySketchError::WidthNotPowerOfTwo { width });
    }
    if width <= 0 {
      return Err(FrequencySketchError::WidthNotPositive { width });
    }
    Ok(Self { width })
  }

  pub(super) const fn get(self) -> i32 {
    self.width
  }

  // The mask that keeps a hash within the row.
  pub(super) const fn index_mask(self) -> i32 {
    self.width - 1
  }
}
