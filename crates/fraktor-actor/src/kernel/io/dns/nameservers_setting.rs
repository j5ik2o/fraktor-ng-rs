//! The configured value for the `nameservers` setting.

use alloc::string::String;
use alloc::vec::Vec;
use core::net::SocketAddr;

use super::{DnsSettings, DnsSettingsError};

/// The raw `nameservers` setting before it is resolved.
///
/// Mirrors the original's `nameservers` key, which is either the string
/// `"default"`, a single address, or a list of addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameserversSetting {
  /// Use the host's name servers.
  Default,
  /// Use the single address written as `host` or `host:port`.
  Single(String),
  /// Use the list of addresses in order.
  List(Vec<String>),
}

impl NameserversSetting {
  /// Resolves the name servers, as the original's `NameServers` does.
  ///
  /// `Default` calls `host_name_servers` and fails with
  /// [`DnsSettingsError::UnableToDetermineDefaultNameservers`] when it returns
  /// `None` or an empty list; no other variant calls it. A single address and
  /// each list element are read with [`DnsSettings::parse_nameserver_address`],
  /// and an empty list is [`DnsSettingsError::EmptyNameservers`].
  pub(crate) fn resolve(
    &self,
    host_name_servers: impl FnOnce() -> Option<Vec<SocketAddr>>,
  ) -> Result<Vec<SocketAddr>, DnsSettingsError> {
    match self {
      | Self::Default => match host_name_servers() {
        | Some(servers) if !servers.is_empty() => Ok(servers),
        | _ => Err(DnsSettingsError::UnableToDetermineDefaultNameservers),
      },
      | Self::Single(text) => Ok(alloc::vec![parse(text)?]),
      | Self::List(texts) if texts.is_empty() => Err(DnsSettingsError::EmptyNameservers),
      | Self::List(texts) => texts.iter().map(|text| parse(text)).collect(),
    }
  }
}

fn parse(text: &str) -> Result<SocketAddr, DnsSettingsError> {
  DnsSettings::parse_nameserver_address(text).map_err(DnsSettingsError::InvalidNameserver)
}
