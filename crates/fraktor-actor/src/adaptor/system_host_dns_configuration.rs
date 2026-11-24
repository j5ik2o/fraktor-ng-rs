//! The host DNS configuration read from the operating system.

use alloc::{string::String, vec::Vec};
use core::net::SocketAddr;
use std::{
  fs,
  path::{Path, PathBuf},
};

use crate::kernel::io::dns::{DnsSettings, HostDnsConfiguration, ResolvConf, ResolvConfParser};

#[cfg(all(test, not(windows)))]
#[path = "system_host_dns_configuration_test.rs"]
mod tests;

// The file both the original's `resolvConf` and the JDK's Unix
// `ResolverConfigurationImpl` read.
const ETC_RESOLV_CONF: &str = "/etc/resolv.conf";

// `ResolverConfigurationImpl.loadConfig` reads `resolvconf("nameserver", 1, 5)`:
// at most one value from each of at most five `nameserver` lines.
const NAMESERVER_KEYWORD: &str = "nameserver";
const MAX_NAMESERVER_LINES: usize = 5;

// The original's `DnsFallbackPort`, used when the `dns://` URI has no port.
const FALLBACK_PORT: u16 = 53;

/// Reads the host's DNS configuration from its `resolv.conf`.
///
/// Mirrors the two host lookups of the original `class DnsSettings` on a Unix
/// host:
///
/// - [`name_servers`](HostDnsConfiguration::name_servers) follows
///   `getDefaultNameServers`. Its JNDI lookup of `dns://` gets the name servers
///   from the JDK's Unix `ResolverConfigurationImpl`, which reads the
///   `nameserver` lines of `/etc/resolv.conf`; each becomes a `dns://` URI whose
///   missing port is 53. The reflection fallback reads the same class, so it is
///   not repeated.
/// - [`resolv_conf`](HostDnsConfiguration::resolv_conf) follows the `lazy val
///   resolvConf`, which parses `/etc/resolv.conf` with `ResolvConfParser`.
///
/// Both return `None` on Windows, where the original skips `resolv.conf` and
/// the JDK reads the name servers from the registry instead; this port does not
/// read the registry. The file is only read, and is read again on each call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemHostDnsConfiguration {
  resolv_conf_path: PathBuf,
}

impl SystemHostDnsConfiguration {
  /// Creates a configuration that reads `/etc/resolv.conf`.
  pub fn new() -> Self {
    Self::reading(ETC_RESOLV_CONF)
  }

  // Reads `path` in place of `/etc/resolv.conf`, so tests can use a file they
  // control.
  pub(crate) fn reading(path: impl AsRef<Path>) -> Self {
    Self { resolv_conf_path: path.as_ref().to_path_buf() }
  }
}

impl Default for SystemHostDnsConfiguration {
  fn default() -> Self {
    Self::new()
  }
}

impl HostDnsConfiguration for SystemHostDnsConfiguration {
  /// Returns the usable `nameserver` entries, or `None` when there are none.
  ///
  /// A file that cannot be read has no entries, as in the JDK, which swallows
  /// the `IOException`. The bytes are decoded leniently, as `FileReader` does.
  /// An entry the original keeps but the kernel's
  /// [`DnsSettings::parse_nameserver_address`] does not accept, a host name or
  /// an IPv6 zone index, is dropped as the original drops an unusable one.
  fn name_servers(&self) -> Option<Vec<SocketAddr>> {
    if cfg!(windows) {
      return None;
    }
    let bytes = fs::read(&self.resolv_conf_path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let servers: Vec<SocketAddr> = nameserver_values(&text).filter_map(server_address).collect();
    (!servers.is_empty()).then_some(servers)
  }

  /// Returns the parsed file, or `None` when it is missing, unreadable, not
  /// UTF-8, or fails to parse.
  ///
  /// The original logs the read or parse failure before returning `None`; this
  /// port has no actor system here to log to, so the failure is not reported.
  fn resolv_conf(&self) -> Option<ResolvConf> {
    if cfg!(windows) {
      return None;
    }
    let bytes = fs::read(&self.resolv_conf_path).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    ResolvConfParser::parse_lines(java_lines(&text)).ok()
  }
}

// `BufferedReader.readLine` and `Files.lines` end a line at `\n`, `\r` or
// `\r\n`. Splitting at each `\r` and `\n` adds empty lines, which both readers
// skip.
fn java_lines(text: &str) -> impl Iterator<Item = &str> {
  text.split(['\n', '\r'])
}

// `ResolverConfigurationImpl.resolvconf("nameserver", 1, 5)`: a line counts when
// it starts with the keyword followed by a space or a tab, and its first value
// is kept, an unbracketed IPv6 literal bracketed. The JDK also drops a value
// that starts with `#` or `;`; such a value has no IP literal before the end
// of the URI authority, so `server_address` drops it here.
fn nameserver_values(text: &str) -> impl Iterator<Item = String> + '_ {
  java_lines(text)
    .filter_map(|line| line.strip_prefix(NAMESERVER_KEYWORD))
    .filter(|rest| rest.starts_with([' ', '\t']))
    .take(MAX_NAMESERVER_LINES)
    .filter_map(|rest| rest.split([' ', '\t']).find(|value| !value.is_empty()))
    .map(bracket_ipv6)
}

// The JDK's rule for an IPv6 literal "in non-BSD-style".
fn bracket_ipv6(value: &str) -> String {
  if value.contains(':') && !value.contains(['.', '[', ']']) {
    format!("[{value}]")
  } else {
    String::from(value)
  }
}

// The original's `asInetSocketAddress(s"dns://$server")`: the URI's host and
// its port, or 53 when the port is missing. The authority ends at the first
// `/`, `?` or `#`, so `192.0.2.1#local` is `192.0.2.1`. The host must be a
// bracketed IPv6 literal or contain no `:`, as a URI server authority
// requires; the host and port are then read by the kernel's name server
// address parser.
fn server_address(value: String) -> Option<SocketAddr> {
  let server = value.split(['/', '?', '#']).next().unwrap_or_default();
  let (host, port) = match server.strip_prefix('[') {
    | Some(_) => {
      let end = server.find(']')? + 1;
      (&server[..end], &server[end..])
    },
    | None => server.find(':').map_or((server, ""), |index| server.split_at(index)),
  };
  let port = match port.strip_prefix(':') {
    | None if port.is_empty() => FALLBACK_PORT,
    | Some("") => FALLBACK_PORT,
    | Some(digits) if digits.bytes().all(|byte| byte.is_ascii_digit()) => digits.parse().ok()?,
    | _ => return None,
  };
  DnsSettings::parse_nameserver_address(&format!("{host}:{port}")).ok()
}
