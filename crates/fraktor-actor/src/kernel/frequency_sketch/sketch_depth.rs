//! The validated number of sketch rows.

use super::FrequencySketchError;

// The number of rows, one hash function each. Zero rows is allowed, as in the
// original: such a sketch counts nothing.
#[derive(Clone, Copy)]
pub(super) struct SketchDepth {
  rows: usize,
}

impl SketchDepth {
  // The four rows of the fixed-shape sketch.
  pub(super) const FOUR: Self = Self { rows: 4 };

  pub(super) fn parse(depth: i32) -> Result<Self, FrequencySketchError> {
    usize::try_from(depth).map(|rows| Self { rows }).map_err(|_| FrequencySketchError::NegativeDepth { depth })
  }

  pub(super) const fn rows(self) -> usize {
    self.rows
  }
}
