//! The configured value for a cache TTL setting.

use core::time::Duration;

/// The raw `positive-ttl` or `negative-ttl` setting before it is resolved.
///
/// Mirrors the original's cache TTL keys, which are `forever`, `never`, or a
/// duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtlSetting {
  /// Cache forever.
  Forever,
  /// Do not cache.
  Never,
  /// Cache for the given duration.
  Duration(Duration),
}
