//! Reads the network adapters from the Windows IP Helper.

#[cfg(windows)]
use alloc::vec;
use alloc::vec::Vec;
#[cfg(windows)]
use core::{
  net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
  ptr,
};

#[cfg(windows)]
use windows_sys::Win32::{
  Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, ERROR_SUCCESS},
  NetworkManagement::{
    IpHelper::{
      GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_FRIENDLY_NAME, GAA_FLAG_SKIP_MULTICAST,
      GAA_FLAG_SKIP_UNICAST, IP_ADAPTER_ADDRESSES_LH,
    },
    Ndis::IfOperStatusUp,
  },
  Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6},
};

use super::windows_adapter::WindowsAdapter;

#[cfg(all(test, windows))]
#[path = "windows_ip_helper_test.rs"]
mod tests;

// The JDK's first buffer size, its growth on `ERROR_BUFFER_OVERFLOW`, and its
// number of retries.
#[cfg(windows)]
const BUFF_SIZE: u32 = 15360;
#[cfg(windows)]
const MAX_TRIES: usize = 3;

// The adapters `GetAdaptersAddresses` reports, or `None` when it fails, as the
// JDK's `getAdapters` does in `ResolverConfigurationImpl.c`. Only the DNS
// server addresses are asked for, and nothing on the host is changed.
#[cfg(windows)]
pub(super) fn read_adapters() -> Option<Vec<WindowsAdapter>> {
  let flags = GAA_FLAG_SKIP_UNICAST | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_FRIENDLY_NAME;
  let mut length = BUFF_SIZE;
  for _ in 0..=MAX_TRIES {
    // `u64` words keep the buffer aligned for `IP_ADAPTER_ADDRESSES_LH`.
    let mut buffer: Vec<u64> = vec![0; (length as usize).div_ceil(8)];
    let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    // SAFETY: `first` points to a writable, aligned buffer of at least
    // `length` bytes, and `length` outlives the call.
    let result = unsafe { GetAdaptersAddresses(u32::from(AF_UNSPEC), flags, ptr::null(), first, &mut length) };
    match result {
      // SAFETY: on success the buffer holds the list the call wrote, and the
      // buffer lives until `adapters` returns.
      | ERROR_SUCCESS => return Some(unsafe { adapters(first) }),
      | ERROR_NO_DATA => return Some(Vec::new()),
      | ERROR_BUFFER_OVERFLOW => length = length.saturating_add(BUFF_SIZE),
      | _ => return None,
    }
  }
  None
}

// The IP Helper exists only on Windows, where `SystemHostDnsConfiguration::new`
// reads it; elsewhere it cannot be read.
#[cfg(not(windows))]
pub(super) fn read_adapters() -> Option<Vec<WindowsAdapter>> {
  None
}

// The adapters of the list that starts at `adapter`.
//
// SAFETY: `adapter` is null or the start of a list `GetAdaptersAddresses`
// wrote, which stays valid while this runs.
#[cfg(windows)]
unsafe fn adapters(mut adapter: *const IP_ADAPTER_ADDRESSES_LH) -> Vec<WindowsAdapter> {
  let mut adapters = Vec::new();
  // SAFETY: each node and each DNS server node is part of the same list.
  while let Some(current) = unsafe { adapter.as_ref() } {
    let mut dns_servers = Vec::new();
    let mut server = current.FirstDnsServerAddress.cast_const();
    while let Some(node) = unsafe { server.as_ref() } {
      if let Some(address) = unsafe { socket_address(node.Address.lpSockaddr) } {
        dns_servers.push(address);
      }
      server = node.Next.cast_const();
    }
    adapters.push(WindowsAdapter::new(current.OperStatus == IfOperStatusUp, dns_servers));
    adapter = current.Next.cast_const();
  }
  adapters
}

// The IPv4 or IPv6 address `sockaddr` holds, with its port and scope ID, or
// `None` for another family.
//
// SAFETY: `sockaddr` is null or points to a socket address of its family's
// size, as `GetAdaptersAddresses` writes it.
#[cfg(windows)]
unsafe fn socket_address(sockaddr: *const SOCKADDR) -> Option<SocketAddr> {
  // SAFETY: the caller's contract; the family says which layout follows.
  let family = unsafe { sockaddr.as_ref() }?.sa_family;
  match family {
    | AF_INET => {
      let v4 = unsafe { &*sockaddr.cast::<SOCKADDR_IN>() };
      let octets = unsafe { v4.sin_addr.S_un.S_addr }.to_ne_bytes();
      Some(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(octets), u16::from_be(v4.sin_port))))
    },
    | AF_INET6 => {
      let v6 = unsafe { &*sockaddr.cast::<SOCKADDR_IN6>() };
      let octets = unsafe { v6.sin6_addr.u.Byte };
      let scope_id = unsafe { v6.Anonymous.sin6_scope_id };
      Some(SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::from(octets),
        u16::from_be(v6.sin6_port),
        u32::from_be(v6.sin6_flowinfo),
        scope_id,
      )))
    },
    | _ => None,
  }
}
