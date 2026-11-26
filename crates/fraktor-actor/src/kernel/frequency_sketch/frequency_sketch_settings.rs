//! The settings of a configurable frequency sketch.

/// The settings [`FrequencySketch::new`](super::FrequencySketch::new) takes
/// next to the capacity.
///
/// Mirrors the named arguments of `FrequencySketch.apply` in
/// `org.apache.pekko.util`, and [`Default`] holds the same defaults. The
/// constructor validates the values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrequencySketchSettings {
  /// The multiplier on the capacity, rounded up to a power of two, that gives
  /// the number of counters per row.
  pub width_multiplier: i32,
  /// The multiplier on the capacity that gives the number of increments after
  /// which every counter is halved.
  pub reset_multiplier: f64,
  /// The number of rows, one hash function each.
  pub depth: i32,
  /// The size of each counter: 2, 4, 8, 16, 32 or 64 bits.
  pub counter_bits: i32,
}

impl Default for FrequencySketchSettings {
  fn default() -> Self {
    Self { width_multiplier: 4, reset_multiplier: 10.0, depth: 4, counter_bits: 4 }
  }
}
