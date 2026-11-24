//! The host DNS configuration the settings read their defaults from.

use alloc::vec::Vec;
use core::net::SocketAddr;

use super::ResolvConf;

/// The DNS configuration of the host the resolver runs on.
///
/// Mirrors the host lookups the original `class DnsSettings` performs: the name
/// servers from JNDI and reflection, and `/etc/resolv.conf`. An implementation
/// that reads the real host is out of scope here.
pub trait HostDnsConfiguration {
  /// Returns the host's name servers, or `None` when none could be determined.
  ///
  /// `None` stands for the original's failure to read the name servers through
  /// either JNDI or reflection.
  fn name_servers(&self) -> Option<Vec<SocketAddr>>;

  /// Returns the parsed `/etc/resolv.conf`, or `None` when there is none.
  ///
  /// `None` stands for the original's cases with no readable `resolv.conf`:
  /// Windows, a missing file, or a parse failure.
  fn resolv_conf(&self) -> Option<ResolvConf>;
}
