//! The configured value for the `ndots` setting.

use super::{DnsSettingsError, ResolvConf};

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

impl NdotsSetting {
  /// Returns whether the setting reads the host's `resolv.conf`.
  pub(crate) fn uses_host(self) -> bool {
    matches!(self, Self::Default)
  }

  /// Resolves the threshold, as the original's `NDots` does.
  ///
  /// `Default` takes `ndots` from `resolv_conf`, or 1 when there is none, and
  /// keeps a negative value as the original does. An explicit negative value is
  /// [`DnsSettingsError::NegativeNdots`].
  pub(crate) fn resolve(self, resolv_conf: Option<&ResolvConf>) -> Result<i32, DnsSettingsError> {
    match self {
      | Self::Default => Ok(resolv_conf.map_or(1, ResolvConf::ndots)),
      | Self::Value(value) if value < 0 => Err(DnsSettingsError::NegativeNdots),
      | Self::Value(value) => Ok(value),
    }
  }
}
