//! A name server address parser.

use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

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
  ///
  /// An IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) is returned as the IPv4
  /// address, as Java's `InetAddress` does for the original's
  /// `InetSocketAddress`. An IPv4 address may pad its four parts with leading
  /// zeros (`008.008.008.008` is `8.8.8.8`), as Java's `InetAddress` reads each
  /// part as a decimal number.
  ///
  /// Text that contains a line terminator (`\n`, `\r`, `U+0085`, `U+2028` or
  /// `U+2029`) is [`NameserverAddressError::Unparseable`]: the `.` of the
  /// original's regular expression does not match a line terminator, so the
  /// whole text does not match and the original throws its
  /// `IllegalArgumentException`.
  pub fn parse_nameserver_address(text: &str) -> Result<SocketAddr, NameserverAddressError> {
    if text.contains(['\n', '\r', '\u{0085}', '\u{2028}', '\u{2029}']) {
      return Err(NameserverAddressError::Unparseable);
    }
    let (host, port) = split_host_and_port(text)?;
    let ip = parse_host(host)?;
    Ok(SocketAddr::new(ip.to_canonical(), port))
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
  if let Some(ipv4) = parse_dotted_decimal_ipv4(host) {
    return Ok(IpAddr::V4(ipv4));
  }
  host.parse::<IpAddr>().map_err(|_| NameserverAddressError::UnsupportedHost)
}

// Java's four-part IPv4 literal: four non-empty runs of ASCII digits separated
// by `.`, each read as a decimal number up to 255. Unlike `Ipv4Addr::from_str`,
// a part may have leading zeros.
fn parse_dotted_decimal_ipv4(host: &str) -> Option<Ipv4Addr> {
  let mut octets = [0_u8; 4];
  let mut parts = host.split('.');
  for octet in &mut octets {
    *octet = parse_decimal_octet(parts.next()?)?;
  }
  if parts.next().is_some() {
    return None;
  }
  Some(Ipv4Addr::from(octets))
}

// One part of a four-part IPv4 literal: one or more ASCII digits, at most 255.
fn parse_decimal_octet(part: &str) -> Option<u8> {
  if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
    return None;
  }
  let digits = part.trim_start_matches('0');
  if digits.is_empty() {
    return Some(0);
  }
  digits.parse::<u8>().ok()
}
