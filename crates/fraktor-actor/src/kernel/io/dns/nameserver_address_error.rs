//! The error a name server address parser reports.

use core::fmt;

/// The reason a name server address could not be parsed.
///
/// Mirrors the failures `org.apache.pekko.io.dns.DnsSettings.parseNameserverAddress`
/// turns into exceptions: a port that is not a valid `u16`, or a host that is not
/// a numeric IP literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameserverAddressError {
  /// The text after the last `:` is a run of digits but not a `u16` port.
  InvalidPort,
  /// The host is not a numeric IPv4 or a bracketed IPv6 address.
  UnsupportedHost,
}

impl fmt::Display for NameserverAddressError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      | Self::InvalidPort => "name server port is not a valid u16",
      | Self::UnsupportedHost => "name server host is not a numeric IP address",
    })
  }
}
