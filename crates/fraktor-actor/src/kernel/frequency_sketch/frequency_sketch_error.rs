//! The error a frequency sketch build reports.

use core::fmt;

/// The reason a frequency sketch constructor rejected its settings.
///
/// Mirrors the `require` checks of `FrequencySketch` and `FastFrequencySketch`
/// in `org.apache.pekko.util`, in the order they run, so the first failure
/// wins. A negative depth, which the original reports as a negative array
/// size, is an error here too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrequencySketchError {
  /// The width multiplier times the capacity rounded up to a power of two
  /// does not fit in an `i32`.
  WidthOverflow {
    /// The computed width.
    width: i64,
  },
  /// The computed width is not a power of two.
  WidthNotPowerOfTwo {
    /// The computed width.
    width: i32,
  },
  /// The computed width is zero or negative, so the sketch has no counters to
  /// index. The original accepts such a width and fails on the first update.
  WidthNotPositive {
    /// The computed width.
    width: i32,
  },
  /// The counter size is not 2, 4, 8, 16, 32 or 64 bits.
  UnsupportedCounterBits {
    /// The requested counter size.
    counter_bits: i32,
  },
  /// The depth is negative.
  NegativeDepth {
    /// The requested depth.
    depth: i32,
  },
}

impl fmt::Display for FrequencySketchError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::WidthOverflow { width } => {
        write!(formatter, "computed width ({width}) exceeds Int.MaxValue; reduce capacity or widthMultiplier")
      },
      | Self::WidthNotPowerOfTwo { width } => write!(formatter, "width must be a power of two, got {width}"),
      | Self::WidthNotPositive { width } => write!(formatter, "width must be positive, got {width}"),
      | Self::UnsupportedCounterBits { counter_bits } => {
        write!(formatter, "counterBits must be 2, 4, 8, 16, 32, or 64 bits, got {counter_bits}")
      },
      | Self::NegativeDepth { depth } => write!(formatter, "depth must not be negative, got {depth}"),
    }
  }
}

impl core::error::Error for FrequencySketchError {}
