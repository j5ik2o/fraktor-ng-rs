// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 870d4499cc #2235; the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/io/dns/DnsSettingsSpec.scala
//
// The original's ten cases under `"DNS settings" must` are kept as one test
// each, named after the case, with the original name as a comment.
//
// The original reads HOCON `Config` and falls back to the spec's own
// `defaultConfig` fixture: `nameservers = "default"`, `resolve-timeout = 1s`,
// `search-domains = []`, `ndots = 1`, `positive-ttl = forever`,
// `negative-ttl = never`, `id-generator-policy = thread-local-random`. That
// fixture is not Pekko's `reference.conf` (whose DNS defaults differ, for
// example `resolve-timeout = 5s` and `id-generator-policy =
// enhanced-double-hash-random`). The port writes the fixture's values as typed
// inputs into `DnsSettingsConfig` through `default_config`.
// Where the original builds `parseString(..).withFallback(defaultConfig)`, the
// written keys win, so the port overrides `default_config` with `with_*`.
//
// The original reads the OS name servers through JNDI and `/etc/resolv.conf`.
// The port passes a host configuration explicitly, so the tests implement
// `HostDnsConfiguration` with controlled values and never read the real host.
//
// The original's `DnsSettings(system, config)` is
// `DnsSettingsValues::new(config, host)` and its accessors map to
// `name_servers`, `search_domains`, `ndots`, `positive_cache_policy`,
// `negative_cache_policy` and `id_generator_policy`.
//
// The port's DNS settings types live in `fraktor_actor::kernel::io::dns`. Until
// they exist the file does not compile: that compile error is the intended red
// for this node.

use core::net::{IpAddr, SocketAddr};
use core::time::Duration;

use fraktor_actor::kernel::io::dns::{
  CachePolicy, DnsSettingsConfig, DnsSettingsValues, HostDnsConfiguration, IdGeneratorPolicy, NameserversSetting,
  NdotsSetting, ResolvConf, SearchDomainsSetting, Ttl, TtlSetting,
};

fn default_config() -> DnsSettingsConfig {
  DnsSettingsConfig::new(
    NameserversSetting::Default,
    Duration::from_secs(1),
    SearchDomainsSetting::List(Vec::new()),
    NdotsSetting::Value(1),
    TtlSetting::Forever,
    TtlSetting::Never,
    "thread-local-random",
  )
}

struct ControlledHost;

impl HostDnsConfiguration for ControlledHost {
  fn name_servers(&self) -> Option<Vec<SocketAddr>> {
    Some(vec![SocketAddr::new("192.0.2.53".parse::<IpAddr>().unwrap(), 53)])
  }

  fn resolv_conf(&self) -> Option<ResolvConf> {
    Some(ResolvConf::new(vec!["host.example".to_string()], 2))
  }
}

fn controlled_host() -> ControlledHost {
  ControlledHost
}

fn settings(config: &DnsSettingsConfig) -> DnsSettingsValues {
  DnsSettingsValues::new(config, &controlled_host()).expect("DNS settings from the controlled host")
}

fn name_server_ips(dns_settings: &DnsSettingsValues) -> Vec<IpAddr> {
  dns_settings.name_servers().iter().map(SocketAddr::ip).collect()
}

// "use host servers if set to default"
#[test]
fn use_host_servers_if_set_to_default() {
  let dns_settings = settings(&default_config());
  let _ = dns_settings.name_servers();
}

// "parse a single name server"
#[test]
fn parse_a_single_name_server() {
  let config = default_config().with_nameservers(NameserversSetting::Single("127.0.0.1".into()));
  let dns_settings = settings(&config);
  assert_eq!(name_server_ips(&dns_settings), vec!["127.0.0.1".parse::<IpAddr>().unwrap()]);
}

// "parse a list of name servers"
#[test]
fn parse_a_list_of_name_servers() {
  let config =
    default_config().with_nameservers(NameserversSetting::List(vec!["127.0.0.1".into(), "127.0.0.2".into()]));
  let dns_settings = settings(&config);
  assert_eq!(
    name_server_ips(&dns_settings),
    vec!["127.0.0.1".parse::<IpAddr>().unwrap(), "127.0.0.2".parse::<IpAddr>().unwrap(),]
  );
}

// "use host search domains if set to default"
//
// The original builds `defaultConfig.withFallback(parseString(..))`, and
// `withFallback` only fills keys `defaultConfig` lacks. `defaultConfig` has
// both `nameservers` and `search-domains`, so the effective config is
// `defaultConfig` itself; the port uses `default_config` unchanged.
#[test]
fn use_host_search_domains_if_set_to_default() {
  let dns_settings = settings(&default_config());
  assert_ne!(dns_settings.search_domains(), ["default"]);
}

// "parse a single search domain"
#[test]
fn parse_a_single_search_domain() {
  let config = default_config()
    .with_nameservers(NameserversSetting::Single("127.0.0.1".into()))
    .with_search_domains(SearchDomainsSetting::Single("example.com".into()));
  let dns_settings = settings(&config);
  assert_eq!(dns_settings.search_domains(), ["example.com"]);
}

// "parse a single list of search domains"
#[test]
fn parse_a_single_list_of_search_domains() {
  let config = default_config()
    .with_nameservers(NameserversSetting::Single("127.0.0.1".into()))
    .with_search_domains(SearchDomainsSetting::List(vec!["example.com".into(), "example.net".into()]));
  let dns_settings = settings(&config);
  assert_eq!(dns_settings.search_domains(), ["example.com", "example.net"]);
}

// "use host ndots if set to default"
#[test]
fn use_host_ndots_if_set_to_default() {
  let config = default_config()
    .with_nameservers(NameserversSetting::Single("127.0.0.1".into()))
    .with_search_domains(SearchDomainsSetting::Single("example.com".into()))
    .with_ndots(NdotsSetting::Default);
  let dns_settings = settings(&config);
  let _ = dns_settings.ndots();
}

// "parse ndots"
#[test]
fn parse_ndots() {
  let config = default_config()
    .with_nameservers(NameserversSetting::Single("127.0.0.1".into()))
    .with_search_domains(SearchDomainsSetting::Single("example.com".into()))
    .with_ndots(NdotsSetting::Value(5));
  let dns_settings = settings(&config);
  assert_eq!(dns_settings.ndots(), 5);
}

// "parse ttl"
#[test]
fn parse_ttl() {
  let defaults = settings(&default_config());
  assert_eq!(defaults.positive_cache_policy(), CachePolicy::Forever);
  assert_eq!(defaults.negative_cache_policy(), CachePolicy::Never);

  let config = default_config()
    .with_positive_ttl(TtlSetting::Duration(Duration::from_secs(10)))
    .with_negative_ttl(TtlSetting::Duration(Duration::from_secs(10 * 24 * 60 * 60)));
  let dns_settings = settings(&config);
  assert_eq!(
    dns_settings.positive_cache_policy(),
    CachePolicy::Ttl(Ttl::from_positive(Duration::from_secs(10)).unwrap())
  );
  assert_eq!(
    dns_settings.negative_cache_policy(),
    CachePolicy::Ttl(Ttl::from_positive(Duration::from_secs(10 * 24 * 60 * 60)).unwrap())
  );
}

// "parse id-generator-policy"
#[test]
fn parse_id_generator_policy() {
  let dns_settings = settings(&default_config());
  assert_eq!(dns_settings.id_generator_policy(), IdGeneratorPolicy::ThreadLocalRandom);
}
