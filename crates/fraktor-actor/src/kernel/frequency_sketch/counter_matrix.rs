//! The packed counter rows both sketches share.

use alloc::{vec, vec::Vec};

use super::{
  counter_bits::CounterBits, reset_size::ResetSize, sketch_depth::SketchDepth, sketch_width::SketchWidth, Bits,
};

// `depth` rows of `width` counters, packed into 64-bit words, with the number
// of increments per row and overall, and the TinyLFU reset.
//
// Both of the original's sketches keep the same matrix and the same reset;
// they differ in how they turn a value into a position in each row. A
// position picks its word by its bits within the width, shifted past the
// counters in one word, and its counter in the word by its low bits, as the
// original's `getCounter` and `incrementCounter` do.
pub(super) struct CounterMatrix {
  rows: Vec<Vec<u64>>,
  row_sizes: Vec<i32>,
  updated_size: i32,
  reset_size: ResetSize,
  column_mask: i32,
  slot_shift: u32,
  slot_mask: i32,
  counter_shift: u32,
  counter_mask: u64,
  odd_mask: u64,
  reset_mask: u64,
}

impl CounterMatrix {
  pub(super) fn new(depth: SketchDepth, width: SketchWidth, counter_bits: CounterBits, reset_size: ResetSize) -> Self {
    let slots = counter_bits.per_word() as i32;
    let row_width = 1.max(width.get() / slots) as usize;
    let counter_mask = counter_bits.max_value();
    let bits = counter_bits.bits();
    // The original folds over `1 to slots` with JVM shifts, whose count is
    // taken modulo 64, so the last step shifts by zero.
    let odd_mask = (1..=slots as u32).fold(1_u64, |mask, count| mask | 1_u64.wrapping_shl(count * bits));
    let counter_reset_mask = counter_mask >> 1;
    let reset_mask =
      (1..=slots as u32).fold(counter_reset_mask, |mask, count| mask | counter_reset_mask.wrapping_shl(count * bits));
    Self {
      // Built row by row: `vec![row; n]` builds `row` even when `n` is zero.
      rows: (0..depth.rows()).map(|_| vec![0; row_width]).collect(),
      row_sizes: vec![0; depth.rows()],
      updated_size: 0,
      reset_size,
      column_mask: width.index_mask(),
      slot_shift: Bits::power_of_two_exponent(slots) as u32,
      slot_mask: slots - 1,
      counter_shift: Bits::power_of_two_exponent(bits as i32) as u32,
      counter_mask,
      odd_mask,
      reset_mask,
    }
  }

  pub(super) const fn depth(&self) -> usize {
    self.row_sizes.len()
  }

  pub(super) const fn size(&self) -> i32 {
    self.updated_size
  }

  // The counter at `position` in `row`, truncated to 32 bits as the
  // original's `toInt` does.
  pub(super) fn counter(&self, row: usize, position: i32) -> i32 {
    let (column, slot) = self.locate(position);
    ((self.rows[row][column] >> slot) & self.counter_mask) as i32
  }

  // Adds one to the counter at `position` in `row` unless it is at its
  // maximum, and returns whether it changed.
  pub(super) fn increment(&mut self, row: usize, position: i32) -> bool {
    let (column, slot) = self.locate(position);
    let mask = self.counter_mask << slot;
    let word = &mut self.rows[row][column];
    if *word & mask == mask {
      return false;
    }
    *word = word.wrapping_add(1 << slot);
    self.row_sizes[row] = self.row_sizes[row].wrapping_add(1);
    true
  }

  // Counts one increment of the sketch, and halves every counter when the
  // count reaches the reset size.
  pub(super) fn record_update(&mut self) {
    self.updated_size = self.updated_size.wrapping_add(1);
    if self.reset_size.is_reached_by(self.updated_size) {
      self.reset();
    }
  }

  fn locate(&self, position: i32) -> (usize, u32) {
    let column = ((position & self.column_mask) as u32 >> self.slot_shift) as usize;
    let slot = ((position & self.slot_mask) as u32) << self.counter_shift;
    (column, slot)
  }

  // The TinyLFU reset. Halving with a shift drops half of every odd counter, so
  // each row's size drops the number of odd counters before it is halved.
  fn reset(&mut self) {
    for (words, row_size) in self.rows.iter_mut().zip(self.row_sizes.iter_mut()) {
      let mut odd: i32 = 0;
      for word in words.iter_mut() {
        odd = odd.wrapping_add((*word & self.odd_mask).count_ones() as i32);
        *word = (*word >> 1) & self.reset_mask;
      }
      *row_size = (row_size.wrapping_sub(odd) as u32 >> 1) as i32;
    }
    if let Some(largest) = self.row_sizes.iter().copied().max() {
      self.updated_size = largest;
    }
  }
}
