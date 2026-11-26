//! The validated size of a sketch counter.

use super::FrequencySketchError;

// The size of one counter in bits: 2, 4, 8, 16, 32 or 64. Counters are packed
// into 64-bit words, so the size divides 64.
#[derive(Clone, Copy)]
pub(super) struct CounterBits {
  bits: u32,
}

impl CounterBits {
  // The four-bit counters of the fixed-shape sketch.
  pub(super) const FOUR: Self = Self { bits: 4 };

  pub(super) const fn parse(counter_bits: i32) -> Result<Self, FrequencySketchError> {
    match counter_bits {
      | 2 | 4 | 8 | 16 | 32 | 64 => Ok(Self { bits: counter_bits as u32 }),
      | _ => Err(FrequencySketchError::UnsupportedCounterBits { counter_bits }),
    }
  }

  pub(super) const fn bits(self) -> u32 {
    self.bits
  }

  // The number of counters in one 64-bit word.
  pub(super) const fn per_word(self) -> u32 {
    u64::BITS / self.bits
  }

  // The largest value a counter holds. A 64-bit counter stops at `i64::MAX`, as
  // the original's `Long.MaxValue` mask does.
  pub(super) const fn max_value(self) -> u64 {
    if self.bits == u64::BITS {
      i64::MAX as u64
    } else {
      (1 << self.bits) - 1
    }
  }
}
