//! The host DNS configuration read from the operating system.

use alloc::{string::String, vec::Vec};
use core::net::SocketAddr;
use std::{
  fs::{self, File},
  io::{BufRead, BufReader},
  path::{Path, PathBuf},
};

use super::UnsupportedHostDnsPlatformError;
use crate::kernel::io::dns::{DnsSettings, HostDnsConfiguration, ResolvConf, ResolvConfParser};

#[cfg(test)]
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
/// Only hosts that keep their resolver configuration in `/etc/resolv.conf` are
/// supported. On Windows the original skips `resolv.conf` and the JDK reads the
/// name servers from the registry; this port does not read the registry, so
/// [`new`](Self::new) fails there with [`UnsupportedHostDnsPlatformError`] and
/// the default name servers are not found automatically (issue #160). Set the
/// `nameservers` setting explicitly, or pass your own [`HostDnsConfiguration`],
/// instead.
///
/// The file is only read, and is read again on each call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemHostDnsConfiguration {
  resolv_conf_path: PathBuf,
}

impl SystemHostDnsConfiguration {
  /// Creates a configuration that reads `/etc/resolv.conf`.
  ///
  /// Returns [`UnsupportedHostDnsPlatformError`] on Windows, whose name servers
  /// are not in `resolv.conf`.
  pub fn new() -> Result<Self, UnsupportedHostDnsPlatformError> {
    Self::for_platform(cfg!(windows))
  }

  // Decides for a Windows host or not, so both outcomes can be tested on any
  // host.
  pub(crate) fn for_platform(windows: bool) -> Result<Self, UnsupportedHostDnsPlatformError> {
    if windows {
      return Err(UnsupportedHostDnsPlatformError::new());
    }
    Ok(Self::reading(ETC_RESOLV_CONF))
  }

  // Reads `path` in place of `/etc/resolv.conf`, so tests can use a file they
  // control.
  pub(crate) fn reading(path: impl AsRef<Path>) -> Self {
    Self { resolv_conf_path: path.as_ref().to_path_buf() }
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
    read_name_servers(BufReader::new(File::open(&self.resolv_conf_path).ok()?))
  }

  /// Returns the parsed file, or `None` when it is missing, unreadable, not
  /// UTF-8, or fails to parse.
  ///
  /// The original logs the read or parse failure before returning `None`; this
  /// port has no actor system here to log to, so the failure is not reported.
  fn resolv_conf(&self) -> Option<ResolvConf> {
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

// The usable name servers in `reader`, or `None` when there are none.
fn read_name_servers(reader: impl BufRead) -> Option<Vec<SocketAddr>> {
  let servers: Vec<SocketAddr> = nameserver_values(lossy_lines(reader)).filter_map(server_address).collect();
  (!servers.is_empty()).then_some(servers)
}

// The lines `BufferedReader.readLine` returns from a `FileReader`, decoded
// leniently, until the first I/O error. The JDK's `resolvconf` catches the
// `IOException` after its loop, so the lines read before it are kept. The
// lines are read lazily, so nothing after the last line used is read.
fn lossy_lines(reader: impl BufRead) -> impl Iterator<Item = String> {
  reader.split(b'\n').map_while(Result::ok).flat_map(|bytes| {
    let text = String::from_utf8_lossy(&bytes).into_owned();
    java_lines(&text).map(String::from).collect::<Vec<_>>()
  })
}

// `ResolverConfigurationImpl.resolvconf("nameserver", 1, 5)`: a line counts when
// it starts with the keyword followed by a space or a tab, up to five such
// lines, and its first value is kept, an unbracketed IPv6 literal bracketed.
// The JDK also drops a value that starts with `#` or `;`; such a value has no
// IP literal before the end of the URI authority, so `server_address` drops it
// here.
fn nameserver_values(lines: impl Iterator<Item = String>) -> impl Iterator<Item = String> {
  lines
    .filter_map(|line| {
      let rest = line.strip_prefix(NAMESERVER_KEYWORD)?;
      rest.starts_with([' ', '\t']).then(|| rest.split([' ', '\t']).find(|value| !value.is_empty()).map(bracket_ipv6))
    })
    .take(MAX_NAMESERVER_LINES)
    .flatten()
}

// The JDK's rule for an IPv6 literal "in non-BSD-style".
fn bracket_ipv6(value: &str) -> String {
  if value.contains(':') && !value.contains(['.', '[', ']']) {
    format!("[{value}]")
  } else {
    String::from(value)
  }
}

// The original's `asInetSocketAddress(s"dns://$server")`: `new URI` must accept
// the text, and the server is the URI's host and port, or port 53 when the
// port is missing. A text `new URI` rejects throws, and the original drops it.
//
// The authority ends at the first `/`, `?` or `#`; what follows must be a valid
// path, query and fragment, so `192.0.2.1#local` is `192.0.2.1` and
// `192.0.2.1#a#b` is dropped. User information before an `@` is skipped. The
// host must be a bracketed IPv6 literal or contain no `:`, as a URI server
// authority requires, and is then read by the kernel's name server address
// parser, so only IP literals are kept.
fn server_address(value: String) -> Option<SocketAddr> {
  let end = value.find(['/', '?', '#']).unwrap_or(value.len());
  let (authority, tail) = value.split_at(end);
  if !is_valid_uri_tail(tail) {
    return None;
  }
  let server = match authority.split_once('@') {
    | Some((user_info, server)) if is_uri_text(user_info, USER_INFO_MARKS) => server,
    | Some(_) => return None,
    | None => authority,
  };
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
  let (host, scope_id) = split_scope_id(host)?;
  let mut address = DnsSettings::parse_nameserver_address(&format!("{host}:{port}")).ok()?;
  if let (SocketAddr::V6(v6), Some(scope_id)) = (&mut address, scope_id) {
    v6.set_scope_id(scope_id);
  }
  Some(address)
}

// Splits `[addr%scope]` into `[addr]` and its numeric scope ID. `new URI`
// accepts a scope ID of letters, digits, `_` and `.`; the original then lets
// `InetAddress` read it as a number or as an interface name. Only a number
// that fits a `u32` is kept here: an interface name would need the host's
// interface table, so it is dropped.
fn split_scope_id(host: &str) -> Option<(String, Option<u32>)> {
  let Some(inner) = host.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) else {
    return Some((String::from(host), None));
  };
  let Some((address, scope)) = inner.split_once('%') else {
    return Some((String::from(host), None));
  };
  if scope.is_empty() || !scope.chars().all(|character| character.is_ascii_alphanumeric() || "_.".contains(character)) {
    return None;
  }
  let scope_id = scope.parse::<u32>().ok()?;
  Some((format!("[{address}]"), Some(scope_id)))
}

// The marks `java.net.URI` allows besides letters, digits and escapes:
// `unreserved` marks plus the extra characters of each component.
const USER_INFO_MARKS: &str = "-_.!~*'();:&=+$,";
const PATH_MARKS: &str = "-_.!~*'():@&=+$,;/";
const QUERY_OR_FRAGMENT_MARKS: &str = "-_.!~*'();/?:@&=+$,[]";

// Whether `tail`, starting at the end of the authority, is an optional path
// (`/...`), then an optional query (`?...`), then an optional fragment
// (`#...`), each holding only the characters `java.net.URI` allows there.
fn is_valid_uri_tail(tail: &str) -> bool {
  let (before_fragment, fragment) = match tail.split_once('#') {
    | Some((before, fragment)) => (before, Some(fragment)),
    | None => (tail, None),
  };
  let (path, query) = match before_fragment.split_once('?') {
    | Some((path, query)) => (path, Some(query)),
    | None => (before_fragment, None),
  };
  is_uri_text(path, PATH_MARKS)
    && query.is_none_or(|query| is_uri_text(query, QUERY_OR_FRAGMENT_MARKS))
    && fragment.is_none_or(|fragment| is_uri_text(fragment, QUERY_OR_FRAGMENT_MARKS))
}

// Letters, digits, `marks`, `%` escapes of two hex digits, and the non-ASCII
// characters `java.net.URI` calls "other" (neither control nor space
// characters).
fn is_uri_text(text: &str, marks: &str) -> bool {
  let mut characters = text.chars();
  while let Some(character) = characters.next() {
    let allowed = match character {
      | '%' => {
        characters.next().is_some_and(|c| c.is_ascii_hexdigit())
          && characters.next().is_some_and(|c| c.is_ascii_hexdigit())
      },
      | c if c.is_ascii() => c.is_ascii_alphanumeric() || marks.contains(c),
      | c => !c.is_control() && !c.is_whitespace(),
    };
    if !allowed {
      return false;
    }
  }
  true
}
