//! The configured value for the `search-domains` setting.

use alloc::string::String;
use alloc::vec::Vec;

use super::ResolvConf;

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

impl SearchDomainsSetting {
  /// Returns whether the setting reads the host's `resolv.conf`.
  pub(crate) fn uses_host(&self) -> bool {
    matches!(self, Self::Default)
  }

  /// Resolves the search domains, as the original's `SearchDomains` does.
  ///
  /// `Default` takes the search list from `resolv_conf`, or an empty list when
  /// there is none. A single domain and a list are used as written.
  pub(crate) fn resolve(&self, resolv_conf: Option<&ResolvConf>) -> Vec<String> {
    match self {
      | Self::Default => resolv_conf.map_or_else(Vec::new, |conf| conf.search().to_vec()),
      | Self::Single(text) => alloc::vec![text.clone()],
      | Self::List(texts) => texts.clone(),
    }
  }
}
