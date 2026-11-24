//! The configured value for the `nameservers` setting.

use alloc::string::String;
use alloc::vec::Vec;

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
