//! A name server address parser.

use core::net::{IpAddr, Ipv6Addr, SocketAddr};

use super::NameserverAddressError;

#[cfg(test)]
#[path = "dns_settings_test.rs"]
mod tests;

// The original's `DnsFallbackPort`.
const FALLBACK_PORT: u16 = 53;

/// Turns a name server address into a socket address.
///
/// Mirrors `org.apache.pekko.io.dns.DnsSettings.parseNameserverAddress`, which
/// matches `"""(.*?)(?::(\d+))?"""` across the whole text and builds an
/// `InetSocketAddress` from the host and the port, or [`FALLBACK_PORT`] when the
/// text has no port. Only numeric IP literals are accepted: a bracketed host is
/// read as an IPv6 address and an unbracketed host as either IPv4 or IPv6. A
/// host name is not resolved; it is reported as
/// [`NameserverAddressError::UnsupportedHost`].
pub struct DnsSettings;

impl DnsSettings {
  /// Parses a name server address of the form `host` or `host:port`.
  ///
  /// The split takes the last `:`: when everything after it is one or more
  /// ASCII digits, the prefix is the host and the suffix is the port; otherwise
  /// the whole text is the host and the port is [`FALLBACK_PORT`]. A port that
  /// is a run of digits but does not fit a `u16` is
  /// [`NameserverAddressError::InvalidPort`].
  pub fn parse_nameserver_address(text: &str) -> Result<SocketAddr, NameserverAddressError> {
    let (host, port) = split_host_and_port(text)?;
    let ip = parse_host(host)?;
    Ok(SocketAddr::new(ip, port))
  }
}

// Splits at the last `:` when the suffix is a non-empty run of ASCII digits.
fn split_host_and_port(text: &str) -> Result<(&str, u16), NameserverAddressError> {
  match text.rfind(':') {
    | Some(index) => {
      let (host, rest) = text.split_at(index);
      let port = &rest[1..];
      if !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) {
        let port = port.parse::<u16>().map_err(|_| NameserverAddressError::InvalidPort)?;
        return Ok((host, port));
      }
      Ok((text, FALLBACK_PORT))
    },
    | None => Ok((text, FALLBACK_PORT)),
  }
}

// Reads a bracketed host as IPv6, anything else as an IPv4 or IPv6 literal.
fn parse_host(host: &str) -> Result<IpAddr, NameserverAddressError> {
  if let Some(inner) = host.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
    return inner.parse::<Ipv6Addr>().map(IpAddr::V6).map_err(|_| NameserverAddressError::UnsupportedHost);
  }
  host.parse::<IpAddr>().map_err(|_| NameserverAddressError::UnsupportedHost)
}
