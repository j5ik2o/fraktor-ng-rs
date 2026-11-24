//! The configured value for the `ndots` setting.

/// The raw `ndots` setting before it is resolved.
///
/// Mirrors the original's `ndots` key, which is either `"default"` or an
/// integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NdotsSetting {
  /// Use `ndots` from the host's `resolv.conf`, or 1 when there is none.
  Default,
  /// Use the given threshold unchanged.
  Value(i32),
}
