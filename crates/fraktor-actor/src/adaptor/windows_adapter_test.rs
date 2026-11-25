//! Unit tests for [`WindowsAdapter`](super::WindowsAdapter).
//!
//! Every test passes adapters it built itself, in place of the ones the IP
//! Helper reports, so the rules run on any host.

use alloc::{format, string::String, vec, vec::Vec};
use core::net::{Ipv6Addr, SocketAddr, SocketAddrV6};

use super::WindowsAdapter;

fn addr(text: &str) -> SocketAddr {
  text.parse().expect("a socket address literal")
}

fn up(dns_servers: &[&str]) -> WindowsAdapter {
  WindowsAdapter::new(true, dns_servers.iter().map(|text| addr(text)).collect())
}

fn down(dns_servers: &[&str]) -> WindowsAdapter {
  WindowsAdapter::new(false, dns_servers.iter().map(|text| addr(text)).collect())
}

// The IP Helper reports a DNS server with port 0; the original's `dns://` URI
// then has no port and the server uses port 53. The servers keep the order of
// the adapters and of each adapter's list.
#[test]
fn the_dns_servers_of_the_adapters_that_are_up_use_port_53() {
  let adapters = [up(&["192.0.2.1:0", "[2001:db8::1]:0"]), up(&["192.0.2.2:0"])];
  assert_eq!(
    WindowsAdapter::name_servers(Some(&adapters)),
    Some(vec![addr("192.0.2.1:53"), addr("[2001:db8::1]:53"), addr("192.0.2.2:53")])
  );
}

// `loadConfig` reads only the adapters whose `OperStatus` is `IfOperStatusUp`.
#[test]
fn the_dns_servers_of_an_adapter_that_is_not_up_are_skipped() {
  let adapters = [down(&["192.0.2.1:0"]), up(&["192.0.2.2:0"]), down(&["192.0.2.3:0"])];
  assert_eq!(WindowsAdapter::name_servers(Some(&adapters)), Some(vec![addr("192.0.2.2:53")]));
}

// `loadConfig` skips an IPv6 DNS server with a scope ID, link-local or
// site-local, and keeps one without.
#[test]
fn an_ipv6_dns_server_with_a_scope_id_is_skipped() {
  let scoped = SocketAddr::V6(SocketAddrV6::new("fe80::1".parse::<Ipv6Addr>().expect("an IPv6 literal"), 0, 0, 7));
  let adapters = [WindowsAdapter::new(true, vec![scoped, addr("[2001:db8::2]:0")])];
  assert_eq!(WindowsAdapter::name_servers(Some(&adapters)), Some(vec![addr("[2001:db8::2]:53")]));
}

// `addressesToList` keeps the first of equal entries, also across adapters.
#[test]
fn a_repeated_dns_server_is_kept_once() {
  let adapters = [up(&["192.0.2.1:0", "[2001:db8::1]:0", "192.0.2.1:0"]), up(&["[2001:db8::1]:0", "192.0.2.2:0"])];
  assert_eq!(
    WindowsAdapter::name_servers(Some(&adapters)),
    Some(vec![addr("192.0.2.1:53"), addr("[2001:db8::1]:53"), addr("192.0.2.2:53")])
  );
}

// `WSAAddressToStringA` writes a port that is not 0, bracketing an IPv6
// address, and the `dns://` URI keeps it. `addressesToList` brackets the IPv4
// text too, as it has a `:`, and `[192.0.2.1:5353]` is not a valid URI host,
// so the original drops it.
#[test]
fn a_dns_server_port_other_than_0_is_kept_for_ipv6_only() {
  let adapters = [up(&["192.0.2.1:5353", "[2001:db8::1]:5353", "192.0.2.2:0"])];
  assert_eq!(
    WindowsAdapter::name_servers(Some(&adapters)),
    Some(vec![addr("[2001:db8::1]:5353"), addr("192.0.2.2:53")])
  );
}

// An IPv4-mapped IPv6 server is read as its IPv4 address, as `InetAddress`
// reads `[::ffff:192.0.2.1]`.
#[test]
fn an_ipv4_mapped_dns_server_is_its_ipv4_address() {
  let adapters = [up(&["[::ffff:192.0.2.1]:0"])];
  assert_eq!(WindowsAdapter::name_servers(Some(&adapters)), Some(vec![addr("192.0.2.1:53")]));
}

// `loadConfig` joins the servers with commas into a buffer of 1024 bytes, and
// `strappend` skips a server that does not fit, then tries the next one.
#[test]
fn a_dns_server_that_does_not_fit_the_native_buffer_is_skipped() {
  // Each address is 39 characters long, so 25 of them with their commas take
  // 999 bytes, and a 26th would need 1040 with the terminating NUL.
  let long: Vec<String> =
    (0..30).map(|index| format!("[fd12:3456:789a:bcde:f012:3456:789a:{:x}]:0", 0x1000 + index)).collect();
  let mut servers: Vec<&str> = long.iter().map(String::as_str).collect();
  servers.push("192.0.2.1:0");
  let adapters = [up(&servers)];
  let mut expected: Vec<SocketAddr> =
    (0..25).map(|index| addr(&format!("[fd12:3456:789a:bcde:f012:3456:789a:{:x}]:53", 0x1000 + index))).collect();
  expected.push(addr("192.0.2.1:53"));
  assert_eq!(WindowsAdapter::name_servers(Some(&adapters)), Some(expected));
}

// No DNS server is no name server, the original's `IllegalStateException` for
// an empty list. The IP Helper's `ERROR_NO_DATA` is no adapter.
#[test]
fn no_dns_server_is_none() {
  assert_eq!(WindowsAdapter::name_servers(Some(&[])), None);
  assert_eq!(WindowsAdapter::name_servers(Some(&[up(&[])])), None);
  assert_eq!(WindowsAdapter::name_servers(Some(&[down(&["192.0.2.1:0"])])), None);
}

// When `GetAdaptersAddresses` fails, `loadConfig` reads nothing and the
// original cannot determine the name servers.
#[test]
fn adapters_that_could_not_be_read_are_none() {
  assert_eq!(WindowsAdapter::name_servers(None), None);
}
