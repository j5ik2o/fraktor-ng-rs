//! Unit tests for [`DnsSettings`](super::DnsSettings).

use core::net::{IpAddr, SocketAddr};

use super::{DnsSettings, NameserverAddressError};

// The four cases the Pekko spec runs, repeated at the unit boundary so that a
// regression here is visible without the integration test.
#[test]
fn an_ipv4_address_with_an_explicit_port_keeps_the_port() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("8.8.8.8:153"),
    Ok(SocketAddr::new("8.8.8.8".parse::<IpAddr>().unwrap(), 153))
  );
}

#[test]
fn a_bracketed_ipv6_address_with_an_explicit_port_keeps_the_port() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("[2001:4860:4860::8888]:153"),
    Ok(SocketAddr::new("2001:4860:4860::8888".parse::<IpAddr>().unwrap(), 153))
  );
}

#[test]
fn an_ipv4_address_without_a_port_gets_the_fallback_port() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("8.8.8.8"),
    Ok(SocketAddr::new("8.8.8.8".parse::<IpAddr>().unwrap(), 53))
  );
}

#[test]
fn a_bracketed_ipv6_address_without_a_port_gets_the_fallback_port() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("[2001:4860:4860::8888]"),
    Ok(SocketAddr::new("2001:4860:4860::8888".parse::<IpAddr>().unwrap(), 53))
  );
}

// Rejects a port above `u16::MAX`. A port read as `u32` or `usize` would accept
// 65536 and `SocketAddr::new` would then panic or truncate.
#[test]
fn a_port_above_the_u16_range_is_rejected() {
  assert_eq!(DnsSettings::parse_nameserver_address("8.8.8.8:65536"), Err(NameserverAddressError::InvalidPort));
}

// Accepts the largest `u16` port, so that 65536 above is the only failing side.
#[test]
fn the_largest_u16_port_is_accepted() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("8.8.8.8:65535"),
    Ok(SocketAddr::new("8.8.8.8".parse::<IpAddr>().unwrap(), 65535))
  );
}

// A host name needs name resolution, which the port does not do; it reports an
// unsupported host instead of resolving or panicking.
#[test]
fn a_host_name_without_a_port_is_unsupported() {
  assert_eq!(DnsSettings::parse_nameserver_address("example.com"), Err(NameserverAddressError::UnsupportedHost));
}

#[test]
fn a_host_name_with_a_port_is_unsupported() {
  assert_eq!(DnsSettings::parse_nameserver_address("example.com:53"), Err(NameserverAddressError::UnsupportedHost));
}

#[test]
fn an_empty_input_is_unsupported() {
  assert_eq!(DnsSettings::parse_nameserver_address(""), Err(NameserverAddressError::UnsupportedHost));
}

// Port zero is in range and is kept; treating it as missing would return 53.
#[test]
fn a_zero_port_is_kept() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("[::1]:0"),
    Ok(SocketAddr::new("::1".parse::<IpAddr>().unwrap(), 0))
  );
}

// The split takes the last `:`, so the trailing `:1` of a bare IPv6 address is
// read as a port and leaves `:` as the host, exactly as the original's regular
// expression does. A split on the first `:` would instead yield a port of 1
// with a different host, or accept the whole address.
#[test]
fn a_bare_ipv6_address_is_split_at_its_last_colon() {
  assert_eq!(DnsSettings::parse_nameserver_address("::1"), Err(NameserverAddressError::UnsupportedHost));
}

// `x` after the last `:` is not a port, so the whole string is the host and it
// is not a numeric IP. A parser that strips a non-numeric port would accept it.
#[test]
fn a_bracketed_ipv6_address_with_a_non_numeric_port_is_unsupported() {
  assert_eq!(DnsSettings::parse_nameserver_address("[2001:db8::1]:x"), Err(NameserverAddressError::UnsupportedHost));
}

// An IPv4 address with more than four octets is not numeric IP; the port split
// must not turn it into a valid address.
#[test]
fn an_ipv4_address_with_an_out_of_range_octet_is_unsupported() {
  assert_eq!(DnsSettings::parse_nameserver_address("8.8.8.256"), Err(NameserverAddressError::UnsupportedHost));
}

// Java's `InetAddress` turns an IPv4-mapped IPv6 literal into an
// `Inet4Address`, so the original returns the same address for
// `[::ffff:192.0.2.53]` as for `192.0.2.53`. Returning the IPv6 form would make
// the two spellings of one server differ.
#[test]
fn an_ipv4_mapped_ipv6_address_is_returned_as_ipv4() {
  let expected = SocketAddr::new("192.0.2.53".parse::<IpAddr>().unwrap(), 53);
  assert_eq!(DnsSettings::parse_nameserver_address("[::ffff:192.0.2.53]"), Ok(expected));
  assert_eq!(DnsSettings::parse_nameserver_address("::ffff:192.0.2.53"), Ok(expected));
  assert_eq!(
    DnsSettings::parse_nameserver_address("[::ffff:192.0.2.53]:153"),
    Ok(SocketAddr::new("192.0.2.53".parse::<IpAddr>().unwrap(), 153))
  );
}

// Only the IPv4-mapped form is converted. An ordinary IPv6 address stays IPv6.
#[test]
fn an_ipv6_address_that_is_not_ipv4_mapped_stays_ipv6() {
  assert_eq!(DnsSettings::parse_nameserver_address("[::1]"), Ok(SocketAddr::new("::1".parse::<IpAddr>().unwrap(), 53)));
}

// Java's `InetAddress` reads each part of a four-part IPv4 literal as a decimal
// number, so the original accepts leading zeros: `008.008.008.008:53` is
// `8.8.8.8:53`. `Ipv4Addr::from_str` rejects leading zeros.
#[test]
fn a_zero_padded_ipv4_address_is_read_as_decimal() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("008.008.008.008:53"),
    Ok(SocketAddr::new("8.8.8.8".parse::<IpAddr>().unwrap(), 53))
  );
  assert_eq!(
    DnsSettings::parse_nameserver_address("010.000.000.001"),
    Ok(SocketAddr::new("10.0.0.1".parse::<IpAddr>().unwrap(), 53))
  );
  assert_eq!(
    DnsSettings::parse_nameserver_address("0255.0.0.0"),
    Ok(SocketAddr::new("255.0.0.0".parse::<IpAddr>().unwrap(), 53))
  );
}

// Leading zeros do not lift the 255 limit of a part, and the literal still
// needs exactly four non-empty parts.
#[test]
fn a_zero_padded_ipv4_address_keeps_the_part_rules() {
  for text in ["0256.0.0.0", "008.008.008", "008.008.008.008.008", "008..008.008", "008.008.008.008."] {
    assert_eq!(DnsSettings::parse_nameserver_address(text), Err(NameserverAddressError::UnsupportedHost), "{text}");
  }
}

// The `.` of the original's regular expression does not match a line
// terminator, so text with one does not match at all and the original throws
// `IllegalArgumentException("Unparseable address string")`. Reading the port
// first would report `8.8.8.8\n:65536` as an invalid port instead.
#[test]
fn text_with_a_line_terminator_is_unparseable() {
  for text in ["8.8.8.8\n:65536", "8.8.8.8\n", "8.8.8.8\r:53", "8.8.8.8\u{0085}", "8.8.8.8\u{2028}", "8.8.8.8\u{2029}"]
  {
    assert_eq!(DnsSettings::parse_nameserver_address(text), Err(NameserverAddressError::Unparseable), "{text:?}");
  }
}

#[test]
fn the_display_text_names_the_failure() {
  let texts = [
    NameserverAddressError::InvalidPort.to_string(),
    NameserverAddressError::UnsupportedHost.to_string(),
    NameserverAddressError::Unparseable.to_string(),
  ];
  for (index, text) in texts.iter().enumerate() {
    assert!(!text.is_empty());
    assert!(texts[index + 1..].iter().all(|other| other != text));
  }
}
