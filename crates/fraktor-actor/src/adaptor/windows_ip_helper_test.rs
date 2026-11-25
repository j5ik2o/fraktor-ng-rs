//! Unit tests for the Windows IP Helper reader.
//!
//! Every test builds the native structures `GetAdaptersAddresses` writes, with
//! known addresses, so the conversion is checked without the host's network
//! configuration.

use alloc::vec;
use core::{
  net::{Ipv6Addr, SocketAddr},
  ptr,
};

use windows_sys::Win32::{
  NetworkManagement::{
    IpHelper::{IP_ADAPTER_ADDRESSES_LH, IP_ADAPTER_DNS_SERVER_ADDRESS_XP},
    Ndis::{IfOperStatusDown, IfOperStatusUp},
  },
  Networking::WinSock::{
    AF_INET, AF_INET6, AF_UNSPEC, IN6_ADDR, IN6_ADDR_0, IN_ADDR, IN_ADDR_0, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6,
    SOCKADDR_IN6_0, SOCKET_ADDRESS,
  },
};

use super::{adapters, socket_address};
use crate::adaptor::windows_adapter::WindowsAdapter;

fn addr(text: &str) -> SocketAddr {
  text.parse().expect("a socket address literal")
}

fn ipv4(octets: [u8; 4], port: u16) -> SOCKADDR_IN {
  SOCKADDR_IN {
    sin_family: AF_INET,
    sin_port: port.to_be(),
    sin_addr: IN_ADDR { S_un: IN_ADDR_0 { S_addr: u32::from_ne_bytes(octets) } },
    ..Default::default()
  }
}

fn ipv6(text: &str, port: u16, scope_id: u32) -> SOCKADDR_IN6 {
  SOCKADDR_IN6 {
    sin6_family: AF_INET6,
    sin6_port: port.to_be(),
    sin6_addr: IN6_ADDR { u: IN6_ADDR_0 { Byte: text.parse::<Ipv6Addr>().expect("an IPv6 literal").octets() } },
    Anonymous: SOCKADDR_IN6_0 { sin6_scope_id: scope_id },
    ..Default::default()
  }
}

fn dns_server(sockaddr: *mut SOCKADDR, length: usize) -> IP_ADAPTER_DNS_SERVER_ADDRESS_XP {
  IP_ADAPTER_DNS_SERVER_ADDRESS_XP {
    Address: SOCKET_ADDRESS {
      lpSockaddr: sockaddr,
      iSockaddrLength: i32::try_from(length).expect("a socket address length"),
    },
    ..Default::default()
  }
}

// The address and port are in network byte order in `SOCKADDR_IN`.
#[test]
fn an_ipv4_socket_address_is_read_in_network_byte_order() {
  let mut sockaddr = ipv4([192, 0, 2, 1], 53);
  let read = unsafe { socket_address(ptr::from_mut(&mut sockaddr).cast()) };
  assert_eq!(read, Some(addr("192.0.2.1:53")));
}

// An IPv6 address keeps its port and scope ID, which `WindowsAdapter` needs.
#[test]
fn an_ipv6_socket_address_keeps_its_port_and_scope_id() {
  let mut sockaddr = ipv6("fe80::1", 5353, 7);
  let read = unsafe { socket_address(ptr::from_mut(&mut sockaddr).cast()) };
  let SocketAddr::V6(read) = read.expect("an IPv6 address") else {
    panic!("not an IPv6 address");
  };
  assert_eq!(read.ip(), &"fe80::1".parse::<Ipv6Addr>().expect("an IPv6 literal"));
  assert_eq!(read.port(), 5353);
  assert_eq!(read.scope_id(), 7);
}

// Another family, and a missing address, are not read.
#[test]
fn another_family_or_no_address_is_none() {
  let mut sockaddr = SOCKADDR { sa_family: AF_UNSPEC, ..Default::default() };
  assert_eq!(unsafe { socket_address(ptr::from_mut(&mut sockaddr)) }, None);
  assert_eq!(unsafe { socket_address(ptr::null()) }, None);
}

// The adapter list and each DNS server list are read in order, with each
// adapter's state.
#[test]
fn the_adapter_list_is_read_in_order() {
  let mut first_v4 = ipv4([192, 0, 2, 1], 0);
  let mut first_v6 = ipv6("2001:db8::1", 0, 0);
  let mut second_v4 = ipv4([192, 0, 2, 2], 0);
  let mut first_v6_node = dns_server(ptr::from_mut(&mut first_v6).cast(), size_of::<SOCKADDR_IN6>());
  let mut first_v4_node = dns_server(ptr::from_mut(&mut first_v4).cast(), size_of::<SOCKADDR_IN>());
  first_v4_node.Next = ptr::from_mut(&mut first_v6_node);
  let mut second_v4_node = dns_server(ptr::from_mut(&mut second_v4).cast(), size_of::<SOCKADDR_IN>());

  let mut second = IP_ADAPTER_ADDRESSES_LH {
    OperStatus: IfOperStatusDown,
    FirstDnsServerAddress: ptr::from_mut(&mut second_v4_node),
    ..Default::default()
  };
  let first = IP_ADAPTER_ADDRESSES_LH {
    OperStatus: IfOperStatusUp,
    FirstDnsServerAddress: ptr::from_mut(&mut first_v4_node),
    Next: ptr::from_mut(&mut second),
    ..Default::default()
  };

  assert_eq!(
    unsafe { adapters(ptr::from_ref(&first)) },
    vec![
      WindowsAdapter::new(true, vec![addr("192.0.2.1:0"), addr("[2001:db8::1]:0")]),
      WindowsAdapter::new(false, vec![addr("192.0.2.2:0")]),
    ]
  );
}

#[test]
fn an_empty_adapter_list_has_no_adapters() {
  assert_eq!(unsafe { adapters(ptr::null()) }, vec![]);
}
