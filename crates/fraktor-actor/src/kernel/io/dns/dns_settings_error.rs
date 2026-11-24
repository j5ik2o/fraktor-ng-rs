//! The error a DNS settings build reports.

use core::fmt;

use super::NameserverAddressError;

/// The reason [`DnsSettingsValues::new`](super::DnsSettingsValues::new)
/// rejected a configuration.
///
/// Mirrors the failures the original `class DnsSettings` raises while resolving
/// its fields, keeping the evaluation order so the first failure wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsSettingsError {
  /// A single name server address did not parse.
  InvalidNameserver(NameserverAddressError),
  /// An explicit `nameservers` list was empty.
  EmptyNameservers,
  /// `nameservers = default` named no server on the host.
  UnableToDetermineDefaultNameservers,
  /// An explicit `ndots` was negative.
  NegativeNdots,
  /// A cache TTL was not positive.
  NonPositiveTtl,
  /// The `id-generator-policy` text named no known policy.
  InvalidIdGeneratorPolicy,
}

impl fmt::Display for DnsSettingsError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::InvalidNameserver(error) => write!(formatter, "invalid name server: {error}"),
      | Self::EmptyNameservers => formatter.write_str("the nameservers list is empty"),
      | Self::UnableToDetermineDefaultNameservers => {
        formatter.write_str("could not determine the default name servers from the host")
      },
      | Self::NegativeNdots => formatter.write_str("the ndots value must not be negative"),
      | Self::NonPositiveTtl => formatter.write_str("the cache time-to-live must be positive"),
      | Self::InvalidIdGeneratorPolicy => formatter.write_str("the id-generator-policy is not recognised"),
    }
  }
}

impl core::error::Error for DnsSettingsError {
  fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
    match self {
      | Self::InvalidNameserver(error) => Some(error),
      | _ => None,
    }
  }
}
