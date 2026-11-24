//! Unit tests for [`DnsSettingsValues`](super::DnsSettingsValues).
//!
//! The original `DnsSettingsSpec` builds settings from HOCON and reads the real
//! host. Here the host is a controlled double so that every branch the original
//! reaches through configuration is reached through `DnsSettingsConfig`, and so
//! that the tests can count how often the host is asked for `/etc/resolv.conf`.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::Cell;
use core::net::{IpAddr, SocketAddr};
use core::time::Duration;

use crate::kernel::io::dns::{
  CachePolicy, DnsSettingsConfig, DnsSettingsError, HostDnsConfiguration, IdGeneratorPolicy, NameserverAddressError,
  NameserversSetting, NdotsSetting, ResolvConf, SearchDomainsSetting, Ttl, TtlSetting,
};

use super::DnsSettingsValues;

// Records how often each host accessor is called, so a test can tell "asked
// once" from "asked twice" and "never asked".
struct ControlledHost {
  name_servers: Option<Vec<SocketAddr>>,
  resolv_conf: Option<ResolvConf>,
  name_server_calls: Cell<usize>,
  resolv_conf_calls: Cell<usize>,
}

impl ControlledHost {
  fn with_defaults() -> Self {
    Self {
      name_servers: Some(vec![socket("192.0.2.53", 53)]),
      resolv_conf: Some(ResolvConf::new(vec![String::from("host.example")], 2)),
      name_server_calls: Cell::new(0),
      resolv_conf_calls: Cell::new(0),
    }
  }

  fn without_name_servers() -> Self {
    let host = Self::with_defaults();
    Self { name_servers: None, ..host }
  }

  fn with_empty_name_servers() -> Self {
    let host = Self::with_defaults();
    Self { name_servers: Some(Vec::new()), ..host }
  }

  fn without_resolv_conf() -> Self {
    let host = Self::with_defaults();
    Self { resolv_conf: None, ..host }
  }

  fn with_resolv_conf(conf: ResolvConf) -> Self {
    let host = Self::with_defaults();
    Self { resolv_conf: Some(conf), ..host }
  }
}

impl HostDnsConfiguration for ControlledHost {
  fn name_servers(&self) -> Option<Vec<SocketAddr>> {
    self.name_server_calls.set(self.name_server_calls.get() + 1);
    self.name_servers.clone()
  }

  fn resolv_conf(&self) -> Option<ResolvConf> {
    self.resolv_conf_calls.set(self.resolv_conf_calls.get() + 1);
    self.resolv_conf.clone()
  }
}

fn socket(text: &str, port: u16) -> SocketAddr {
  SocketAddr::new(text.parse::<IpAddr>().expect("an IP literal"), port)
}

fn base_config() -> DnsSettingsConfig {
  DnsSettingsConfig::new(
    NameserversSetting::Default,
    Duration::from_secs(1),
    SearchDomainsSetting::Default,
    NdotsSetting::Default,
    TtlSetting::Forever,
    TtlSetting::Never,
    "thread-local-random",
  )
}

fn build(config: &DnsSettingsConfig, host: &ControlledHost) -> Result<DnsSettingsValues, DnsSettingsError> {
  DnsSettingsValues::new(config, host)
}

fn name_server_ips(values: &DnsSettingsValues) -> Vec<IpAddr> {
  values.name_servers().iter().map(SocketAddr::ip).collect()
}

// `Default` uses the host's name servers; the values come from the host, not
// from anything the config holds.
#[test]
fn default_nameservers_come_from_the_host() {
  let host = ControlledHost::with_defaults();
  let values = build(&base_config(), &host).expect("settings");
  assert_eq!(name_server_ips(&values), vec!["192.0.2.53".parse::<IpAddr>().unwrap()]);
}

// The original reports a failure when the host cannot provide name servers at
// all, so `None` is a failure rather than an empty list.
#[test]
fn default_nameservers_fail_when_the_host_has_none() {
  let host = ControlledHost::without_name_servers();
  assert_eq!(build(&base_config(), &host), Err(DnsSettingsError::UnableToDetermineDefaultNameservers));
}

// An empty list from the host is the same failure as `None`: the settings
// cannot name a server.
#[test]
fn default_nameservers_fail_when_the_host_list_is_empty() {
  let host = ControlledHost::with_empty_name_servers();
  assert_eq!(build(&base_config(), &host), Err(DnsSettingsError::UnableToDetermineDefaultNameservers));
}

#[test]
fn a_single_name_server_is_parsed() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_nameservers(NameserversSetting::Single(String::from("127.0.0.1")));
  let values = build(&config, &host).expect("settings");
  assert_eq!(name_server_ips(&values), vec!["127.0.0.1".parse::<IpAddr>().unwrap()]);
}

#[test]
fn a_list_of_name_servers_is_parsed_in_order() {
  let host = ControlledHost::with_defaults();
  let config = base_config()
    .with_nameservers(NameserversSetting::List(vec![String::from("127.0.0.1"), String::from("127.0.0.2")]));
  let values = build(&config, &host).expect("settings");
  assert_eq!(
    name_server_ips(&values),
    vec!["127.0.0.1".parse::<IpAddr>().unwrap(), "127.0.0.2".parse::<IpAddr>().unwrap()]
  );
}

// A bracketed IPv6 literal with an explicit port goes through the existing
// parser unchanged.
#[test]
fn a_bracketed_ipv6_name_server_keeps_its_port() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_nameservers(NameserversSetting::Single(String::from("[::1]:5353")));
  let values = build(&config, &host).expect("settings");
  assert_eq!(values.name_servers(), [socket("::1", 5353)]);
}

// A host name is not a numeric IP, so it is an invalid name server that wraps
// the parser's error.
#[test]
fn a_host_name_name_server_is_invalid() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_nameservers(NameserversSetting::Single(String::from("example.com")));
  assert_eq!(build(&config, &host), Err(DnsSettingsError::InvalidNameserver(NameserverAddressError::UnsupportedHost)));
}

// In a list the first failure is returned, whether or not later entries would
// parse.
#[test]
fn a_late_invalid_name_server_in_a_list_fails() {
  let host = ControlledHost::with_defaults();
  let config = base_config()
    .with_nameservers(NameserversSetting::List(vec![String::from("127.0.0.1"), String::from("not-an-ip")]));
  assert_eq!(build(&config, &host), Err(DnsSettingsError::InvalidNameserver(NameserverAddressError::UnsupportedHost)));
}

// An empty list names nothing, which the original reports separately from a
// single invalid entry.
#[test]
fn an_empty_name_server_list_is_empty() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_nameservers(NameserversSetting::List(Vec::new()));
  assert_eq!(build(&config, &host), Err(DnsSettingsError::EmptyNameservers));
}

// An explicit nameserver means the host is never asked, even for the default
// resolv.conf; this also guards against resolving `Default` eagerly.
#[test]
fn explicit_nameservers_do_not_ask_the_host() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_nameservers(NameserversSetting::Single(String::from("127.0.0.1")));
  let _ = build(&config, &host).expect("settings");
  assert_eq!(host.name_server_calls.get(), 0);
}

#[test]
fn the_resolve_timeout_is_kept() {
  let host = ControlledHost::with_defaults();
  let config = DnsSettingsConfig::new(
    NameserversSetting::Default,
    Duration::from_millis(250),
    SearchDomainsSetting::Default,
    NdotsSetting::Default,
    TtlSetting::Forever,
    TtlSetting::Never,
    "thread-local-random",
  );
  let values = build(&config, &host).expect("settings");
  assert_eq!(values.resolve_timeout(), Duration::from_millis(250));
}

#[test]
fn default_search_domains_come_from_the_host() {
  let host = ControlledHost::with_defaults();
  let values = build(&base_config(), &host).expect("settings");
  assert_eq!(values.search_domains(), ["host.example"]);
}

#[test]
fn default_ndots_comes_from_the_host() {
  let host = ControlledHost::with_defaults();
  let values = build(&base_config(), &host).expect("settings");
  assert_eq!(values.ndots(), 2);
}

// The original's "use host ndots if set to default" case writes the search
// domain and leaves only ndots as default; that one default must still read the
// host's resolv.conf, once.
#[test]
fn a_default_ndots_beside_explicit_search_domains_comes_from_the_host() {
  let host = ControlledHost::with_defaults();
  let config = base_config()
    .with_nameservers(NameserversSetting::Single(String::from("127.0.0.1")))
    .with_search_domains(SearchDomainsSetting::Single(String::from("example.com")));
  let values = build(&config, &host).expect("settings");
  assert_eq!(values.search_domains(), ["example.com"]);
  assert_eq!(values.ndots(), 2);
  assert_eq!(host.resolv_conf_calls.get(), 1);
}

// Without a resolv.conf the search list is empty and ndots falls back to 1,
// matching the original's empty defaults.
#[test]
fn a_missing_resolv_conf_gives_empty_search_domains_and_ndots_one() {
  let host = ControlledHost::without_resolv_conf();
  let values = build(&base_config(), &host).expect("settings");
  assert_eq!(values.search_domains(), Vec::<String>::new().as_slice());
  assert_eq!(values.ndots(), 1);
}

// A negative ndots from resolv.conf is kept: the original validates only the
// explicitly set value.
#[test]
fn a_negative_ndots_from_resolv_conf_is_kept() {
  let host = ControlledHost::with_resolv_conf(ResolvConf::new(Vec::new(), -1));
  let values = build(&base_config(), &host).expect("settings");
  assert_eq!(values.ndots(), -1);
}

#[test]
fn a_single_search_domain_is_used_verbatim() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_search_domains(SearchDomainsSetting::Single(String::from("example.com")));
  let values = build(&config, &host).expect("settings");
  assert_eq!(values.search_domains(), ["example.com"]);
}

// The literal element `"default"` is kept as written; the original does not
// treat it as a keyword inside a list.
#[test]
fn a_search_domain_list_is_used_verbatim_including_default() {
  let host = ControlledHost::with_defaults();
  let config = base_config()
    .with_search_domains(SearchDomainsSetting::List(vec![String::from("default"), String::from("example.net")]));
  let values = build(&config, &host).expect("settings");
  assert_eq!(values.search_domains(), ["default", "example.net"]);
}

#[test]
fn explicit_search_domains_do_not_ask_the_host() {
  let host = ControlledHost::with_defaults();
  let config = base_config()
    .with_nameservers(NameserversSetting::Single(String::from("127.0.0.1")))
    .with_search_domains(SearchDomainsSetting::Single(String::from("example.com")))
    .with_ndots(NdotsSetting::Value(3));
  let _ = build(&config, &host).expect("settings");
  assert_eq!(host.resolv_conf_calls.get(), 0);
}

// The original reads resolv.conf through a `lazy val`, so two defaults share
// one read. Reading it once per consumer would double the call.
#[test]
fn both_resolv_conf_defaults_ask_the_host_once() {
  let host = ControlledHost::with_defaults();
  let _ = build(&base_config(), &host).expect("settings");
  assert_eq!(host.resolv_conf_calls.get(), 1);
}

// One default alone still reads resolv.conf; only the shared read matters, not
// that both were default.
#[test]
fn one_resolv_conf_default_asks_the_host_once() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_ndots(NdotsSetting::Value(1));
  let values = build(&config, &host).expect("settings");
  assert_eq!(values.search_domains(), ["host.example"]);
  assert_eq!(host.resolv_conf_calls.get(), 1);
}

#[test]
fn an_explicit_ndots_value_is_kept() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_ndots(NdotsSetting::Value(5));
  let values = build(&config, &host).expect("settings");
  assert_eq!(values.ndots(), 5);
}

// Zero is a legal threshold in the original; it must not be confused with "not
// set" and replaced by the default 1.
#[test]
fn an_explicit_ndots_of_zero_is_kept() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_ndots(NdotsSetting::Value(0));
  let values = build(&config, &host).expect("settings");
  assert_eq!(values.ndots(), 0);
}

#[test]
fn a_negative_explicit_ndots_is_rejected() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_ndots(NdotsSetting::Value(-1));
  assert_eq!(build(&config, &host), Err(DnsSettingsError::NegativeNdots));
}

#[test]
fn forever_and_never_are_kept_as_cache_policies() {
  let host = ControlledHost::with_defaults();
  let values = build(&base_config(), &host).expect("settings");
  assert_eq!(values.positive_cache_policy(), CachePolicy::Forever);
  assert_eq!(values.negative_cache_policy(), CachePolicy::Never);
}

#[test]
fn a_positive_duration_becomes_a_ttl_cache_policy() {
  let host = ControlledHost::with_defaults();
  let config = base_config()
    .with_positive_ttl(TtlSetting::Duration(Duration::from_secs(10)))
    .with_negative_ttl(TtlSetting::Duration(Duration::from_secs(20)));
  let values = build(&config, &host).expect("settings");
  assert_eq!(values.positive_cache_policy(), CachePolicy::Ttl(Ttl::from_positive(Duration::from_secs(10)).unwrap()));
  assert_eq!(values.negative_cache_policy(), CachePolicy::Ttl(Ttl::from_positive(Duration::from_secs(20)).unwrap()));
}

// A zero positive TTL is non-positive and rejected.
#[test]
fn a_zero_positive_ttl_is_rejected() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_positive_ttl(TtlSetting::Duration(Duration::ZERO));
  assert_eq!(build(&config, &host), Err(DnsSettingsError::NonPositiveTtl));
}

// A zero negative TTL is rejected the same way, at its own step rather than
// being shared with the positive TTL.
#[test]
fn a_zero_negative_ttl_is_rejected() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_negative_ttl(TtlSetting::Duration(Duration::ZERO));
  assert_eq!(build(&config, &host), Err(DnsSettingsError::NonPositiveTtl));
}

fn with_policy(policy: &str) -> DnsSettingsConfig {
  DnsSettingsConfig::new(
    NameserversSetting::Default,
    Duration::from_secs(1),
    SearchDomainsSetting::Default,
    NdotsSetting::Default,
    TtlSetting::Forever,
    TtlSetting::Never,
    policy,
  )
}

#[test]
fn thread_local_random_text_selects_thread_local_random() {
  let host = ControlledHost::with_defaults();
  let values = build(&with_policy("thread-local-random"), &host).expect("settings");
  assert_eq!(values.id_generator_policy(), IdGeneratorPolicy::ThreadLocalRandom);
}

#[test]
fn secure_random_text_selects_secure_random() {
  let host = ControlledHost::with_defaults();
  let values = build(&with_policy("secure-random"), &host).expect("settings");
  assert_eq!(values.id_generator_policy(), IdGeneratorPolicy::SecureRandom);
}

// The empty string and the spelled-out name both select the legacy policy the
// port only identifies but does not implement.
#[test]
fn empty_text_selects_enhanced_double_hash_random() {
  let host = ControlledHost::with_defaults();
  let values = build(&with_policy(""), &host).expect("settings");
  assert_eq!(values.id_generator_policy(), IdGeneratorPolicy::EnhancedDoubleHashRandom);
}

#[test]
fn enhanced_double_hash_random_text_selects_enhanced_double_hash_random() {
  let host = ControlledHost::with_defaults();
  let values = build(&with_policy("enhanced-double-hash-random"), &host).expect("settings");
  assert_eq!(values.id_generator_policy(), IdGeneratorPolicy::EnhancedDoubleHashRandom);
}

// Matching is case-sensitive; a differently cased name is not recognised.
#[test]
fn a_mis_cased_policy_text_is_rejected() {
  let host = ControlledHost::with_defaults();
  assert_eq!(build(&with_policy("Thread-Local-Random"), &host), Err(DnsSettingsError::InvalidIdGeneratorPolicy));
}

#[test]
fn an_unknown_policy_text_is_rejected() {
  let host = ControlledHost::with_defaults();
  assert_eq!(build(&with_policy("random"), &host), Err(DnsSettingsError::InvalidIdGeneratorPolicy));
}

// The original evaluates the fields in order and returns the first failure.
// Both an invalid nameserver and an invalid policy are present; the nameserver
// comes first, so its failure must win.
#[test]
fn a_nameserver_failure_wins_over_an_id_policy_failure() {
  let host = ControlledHost::with_defaults();
  let config = DnsSettingsConfig::new(
    NameserversSetting::Single(String::from("example.com")),
    Duration::from_secs(1),
    SearchDomainsSetting::Default,
    NdotsSetting::Default,
    TtlSetting::Forever,
    TtlSetting::Never,
    "random",
  );
  assert_eq!(build(&config, &host), Err(DnsSettingsError::InvalidNameserver(NameserverAddressError::UnsupportedHost)));
}

// A TTL failure comes before a search/ndots failure, so the nameservers and
// TTLs are settled before the host is asked for resolv.conf.
#[test]
fn a_ttl_failure_comes_before_a_later_ndots_failure() {
  let host = ControlledHost::with_defaults();
  let config =
    base_config().with_ndots(NdotsSetting::Value(-5)).with_positive_ttl(TtlSetting::Duration(Duration::ZERO));
  assert_eq!(build(&config, &host), Err(DnsSettingsError::NonPositiveTtl));
}

// `InvalidNameserver` keeps the parser error as its source so a caller can read
// the precise reason.
#[test]
fn an_invalid_nameserver_exposes_the_parser_error_as_source() {
  let host = ControlledHost::with_defaults();
  let config = base_config().with_nameservers(NameserversSetting::Single(String::from("example.com")));
  let error = build(&config, &host).expect_err("an invalid nameserver");
  assert_eq!(
    core::error::Error::source(&error).expect("a parser error as source").to_string(),
    NameserverAddressError::UnsupportedHost.to_string()
  );
}
