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

#[test]
fn the_display_text_names_the_failure() {
  assert!(!NameserverAddressError::InvalidPort.to_string().is_empty());
  assert!(!NameserverAddressError::UnsupportedHost.to_string().is_empty());
  assert_ne!(NameserverAddressError::InvalidPort.to_string(), NameserverAddressError::UnsupportedHost.to_string());
}
