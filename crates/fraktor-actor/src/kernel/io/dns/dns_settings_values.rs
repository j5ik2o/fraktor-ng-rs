//! The DNS settings resolved from a configuration and the host.

use alloc::string::String;
use alloc::vec::Vec;
use core::net::SocketAddr;
use core::time::Duration;

use super::{
  CachePolicy, DnsSettings, DnsSettingsConfig, DnsSettingsError, HostDnsConfiguration, IdGeneratorPolicy,
  NameserversSetting, NdotsSetting, ResolvConf, SearchDomainsSetting, Ttl, TtlSetting,
};

#[cfg(test)]
#[path = "dns_settings_values_test.rs"]
mod tests;

/// The resolved DNS settings a resolver uses.
///
/// Mirrors the fields of the original `class DnsSettings`, kept apart from the
/// [`DnsSettings`] name-server parser, which stands for the original's
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
    let name_servers = resolve_name_servers(config.nameservers(), host)?;
    let resolve_timeout = config.resolve_timeout();
    let positive_cache_policy = resolve_cache_policy(config.positive_ttl())?;
    let negative_cache_policy = resolve_cache_policy(config.negative_ttl())?;

    let resolv_conf =
      if needs_resolv_conf(config.search_domains(), config.ndots()) { host.resolv_conf() } else { None };
    let search_domains = resolve_search_domains(config.search_domains(), resolv_conf.as_ref());
    let ndots = resolve_ndots(config.ndots(), resolv_conf.as_ref())?;
    let id_generator_policy = resolve_id_generator_policy(config.id_generator_policy())?;

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

fn needs_resolv_conf(search_domains: &SearchDomainsSetting, ndots: NdotsSetting) -> bool {
  matches!(search_domains, SearchDomainsSetting::Default) || matches!(ndots, NdotsSetting::Default)
}

fn resolve_name_servers<H: HostDnsConfiguration + ?Sized>(
  setting: &NameserversSetting,
  host: &H,
) -> Result<Vec<SocketAddr>, DnsSettingsError> {
  match setting {
    | NameserversSetting::Default => match host.name_servers() {
      | Some(servers) if !servers.is_empty() => Ok(servers),
      | _ => Err(DnsSettingsError::UnableToDetermineDefaultNameservers),
    },
    | NameserversSetting::Single(text) => {
      let address = DnsSettings::parse_nameserver_address(text).map_err(DnsSettingsError::InvalidNameserver)?;
      Ok(alloc::vec![address])
    },
    | NameserversSetting::List(texts) => {
      if texts.is_empty() {
        return Err(DnsSettingsError::EmptyNameservers);
      }
      texts
        .iter()
        .map(|text| DnsSettings::parse_nameserver_address(text).map_err(DnsSettingsError::InvalidNameserver))
        .collect()
    },
  }
}

fn resolve_cache_policy(setting: TtlSetting) -> Result<CachePolicy, DnsSettingsError> {
  match setting {
    | TtlSetting::Forever => Ok(CachePolicy::Forever),
    | TtlSetting::Never => Ok(CachePolicy::Never),
    | TtlSetting::Duration(duration) => {
      Ttl::from_positive(duration).map(CachePolicy::Ttl).map_err(|_| DnsSettingsError::NonPositiveTtl)
    },
  }
}

fn resolve_search_domains(setting: &SearchDomainsSetting, resolv_conf: Option<&ResolvConf>) -> Vec<String> {
  match setting {
    | SearchDomainsSetting::Default => resolv_conf.map_or_else(Vec::new, |conf| conf.search().to_vec()),
    | SearchDomainsSetting::Single(text) => alloc::vec![text.clone()],
    | SearchDomainsSetting::List(texts) => texts.clone(),
  }
}

fn resolve_ndots(setting: NdotsSetting, resolv_conf: Option<&ResolvConf>) -> Result<i32, DnsSettingsError> {
  match setting {
    | NdotsSetting::Default => Ok(resolv_conf.map_or(1, |conf| conf.ndots())),
    | NdotsSetting::Value(value) if value < 0 => Err(DnsSettingsError::NegativeNdots),
    | NdotsSetting::Value(value) => Ok(value),
  }
}

fn resolve_id_generator_policy(text: &str) -> Result<IdGeneratorPolicy, DnsSettingsError> {
  match text {
    | "thread-local-random" => Ok(IdGeneratorPolicy::ThreadLocalRandom),
    | "secure-random" => Ok(IdGeneratorPolicy::SecureRandom),
    | "" | "enhanced-double-hash-random" => Ok(IdGeneratorPolicy::EnhancedDoubleHashRandom),
    | _ => Err(DnsSettingsError::InvalidIdGeneratorPolicy),
  }
}
