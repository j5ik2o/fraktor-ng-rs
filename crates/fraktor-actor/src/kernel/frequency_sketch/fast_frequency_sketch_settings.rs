//! The settings of the fixed-shape frequency sketch.

/// The settings [`FastFrequencySketch::new`](super::FastFrequencySketch::new)
/// takes next to the capacity.
///
/// Mirrors the named arguments of `FastFrequencySketch.apply` in
/// `org.apache.pekko.util`, and [`Default`] holds the same defaults. The depth
/// is fixed at four rows and the counters at four bits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FastFrequencySketchSettings {
  /// The multiplier on the capacity, rounded up to a power of two, that gives
  /// the number of counters per row.
  pub width_multiplier: i32,
  /// The multiplier on the capacity that gives the number of increments after
  /// which every counter is halved.
  pub reset_multiplier: f64,
}

impl Default for FastFrequencySketchSettings {
  fn default() -> Self {
    Self { width_multiplier: 4, reset_multiplier: 10.0 }
  }
}
