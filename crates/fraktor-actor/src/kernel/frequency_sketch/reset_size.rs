//! The number of increments after which a sketch halves its counters.

// The size at which the TinyLFU reset runs. A size the sketch cannot reach,
// such as zero or a negative value, disables the reset, as in the original.
#[derive(Clone, Copy)]
pub(super) struct ResetSize {
  size: i32,
}

impl ResetSize {
  // The original's `math.min(Int.MaxValue.toLong, (resetMultiplier * capacity).toLong).toInt`.
  // The float conversion saturates and maps NaN to zero, and the final
  // conversion keeps the low 32 bits, both as on the JVM.
  pub(super) fn of(capacity: i32, reset_multiplier: f64) -> Self {
    let wide = i64::from(i32::MAX).min((reset_multiplier * f64::from(capacity)) as i64);
    Self { size: wide as i32 }
  }

  pub(super) const fn is_reached_by(self, size: i32) -> bool {
    size == self.size
  }
}
