//! The configured value for the `search-domains` setting.

use alloc::string::String;
use alloc::vec::Vec;

/// The raw `search-domains` setting before it is resolved.
///
/// Mirrors the original's `search-domains` key, which is either the string
/// `"default"`, a single domain, or a list of domains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchDomainsSetting {
  /// Use the search list from the host's `resolv.conf`.
  Default,
  /// Use the single search domain.
  Single(String),
  /// Use the listed search domains in order.
  List(Vec<String>),
}
