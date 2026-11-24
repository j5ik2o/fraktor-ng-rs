//! The DNS settings resolved from a configuration and the host.

use alloc::string::String;
use alloc::vec::Vec;
use core::net::SocketAddr;
use core::time::Duration;

use super::{CachePolicy, DnsSettingsConfig, DnsSettingsError, HostDnsConfiguration, IdGeneratorPolicy};

#[cfg(test)]
#[path = "dns_settings_values_test.rs"]
mod tests;

/// The resolved DNS settings a resolver uses.
///
/// Mirrors the fields of the original `class DnsSettings`, kept apart from the
/// [`DnsSettings`](super::DnsSettings) name-server parser, which stands for the original's
/// `object DnsSettings`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsSettingsValues {
  name_servers: Vec<SocketAddr>,
  resolve_timeout: Duration,
  positive_cache_policy: CachePolicy,
  negative_cache_policy: CachePolicy,
  id_generator_policy: IdGeneratorPolicy,
  search_domains: Vec<String>,
  ndots: i32,
}

impl DnsSettingsValues {
  /// Resolves every setting, in the original's field order, returning the first
  /// failure.
  ///
  /// The original's `IdGeneratorPolicy` is a `lazy val` checked on first read;
  /// this port checks it last, after the other failures. The `resolv.conf` is
  /// read at most once, as the original's `lazy val resolvConf` does, and only
  /// when the search domains or `ndots` are `default`.
  pub fn new<H: HostDnsConfiguration + ?Sized>(config: &DnsSettingsConfig, host: &H) -> Result<Self, DnsSettingsError> {
    let name_servers = config.nameservers().resolve(|| host.name_servers())?;
    let resolve_timeout = config.resolve_timeout();
    let positive_cache_policy = config.positive_ttl().cache_policy().map_err(|_| DnsSettingsError::NonPositiveTtl)?;
    let negative_cache_policy = config.negative_ttl().cache_policy().map_err(|_| DnsSettingsError::NonPositiveTtl)?;

    let resolv_conf =
      if config.search_domains().uses_host() || config.ndots().uses_host() { host.resolv_conf() } else { None };
    let search_domains = config.search_domains().resolve(resolv_conf.as_ref());
    let ndots = config.ndots().resolve(resolv_conf.as_ref())?;
    let id_generator_policy = IdGeneratorPolicy::from_config_value(config.id_generator_policy())
      .ok_or(DnsSettingsError::InvalidIdGeneratorPolicy)?;

    Ok(Self {
      name_servers,
      resolve_timeout,
      positive_cache_policy,
      negative_cache_policy,
      id_generator_policy,
      search_domains,
      ndots,
    })
  }

  /// Returns the resolved name servers.
  pub fn name_servers(&self) -> &[SocketAddr] {
    &self.name_servers
  }

  /// Returns the resolve timeout.
  pub fn resolve_timeout(&self) -> Duration {
    self.resolve_timeout
  }

  /// Returns the positive cache policy.
  pub fn positive_cache_policy(&self) -> CachePolicy {
    self.positive_cache_policy
  }

  /// Returns the negative cache policy.
  pub fn negative_cache_policy(&self) -> CachePolicy {
    self.negative_cache_policy
  }

  /// Returns the resolved ID generator policy.
  pub fn id_generator_policy(&self) -> IdGeneratorPolicy {
    self.id_generator_policy
  }

  /// Returns the resolved search domains.
  pub fn search_domains(&self) -> &[String] {
    &self.search_domains
  }

  /// Returns the resolved `ndots` threshold.
  pub fn ndots(&self) -> i32 {
    self.ndots
  }
}
