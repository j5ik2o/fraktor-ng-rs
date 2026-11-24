//! Unit tests for [`SystemHostDnsConfiguration`](super::SystemHostDnsConfiguration).
//!
//! Every test reads a file it wrote under a fresh temporary directory; none of
//! them reads or changes the real `/etc/resolv.conf`.

use alloc::{
  format,
  string::{String, ToString},
  vec,
  vec::Vec,
};
use core::{
  net::SocketAddr,
  sync::atomic::{AtomicUsize, Ordering},
  time::Duration,
};
use std::{
  fs,
  path::{Path, PathBuf},
};

use super::SystemHostDnsConfiguration;
use crate::kernel::io::dns::{
  DnsSettingsConfig, DnsSettingsError, DnsSettingsValues, HostDnsConfiguration, NameserversSetting, NdotsSetting,
  ResolvConf, SearchDomainsSetting, TtlSetting,
};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

// A temporary directory removed when the test ends.
struct TempDir(PathBuf);

impl TempDir {
  fn new() -> Self {
    let index = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("fraktor-host-dns-{}-{index}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create a temporary directory");
    Self(path)
  }

  fn path(&self) -> &Path {
    &self.0
  }

  fn file(&self, contents: &[u8]) -> PathBuf {
    let path = self.0.join("resolv.conf");
    fs::write(&path, contents).expect("write a temporary resolv.conf");
    path
  }
}

impl Drop for TempDir {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

fn host_with(contents: &str) -> (TempDir, SystemHostDnsConfiguration) {
  let dir = TempDir::new();
  let host = SystemHostDnsConfiguration::reading(dir.file(contents.as_bytes()));
  (dir, host)
}

fn addr(text: &str) -> SocketAddr {
  text.parse().expect("a socket address literal")
}

fn strings(items: &[&str]) -> Vec<String> {
  items.iter().map(ToString::to_string).collect()
}

fn default_config() -> DnsSettingsConfig {
  DnsSettingsConfig::new(
    NameserversSetting::Default,
    Duration::from_secs(5),
    SearchDomainsSetting::Default,
    NdotsSetting::Default,
    TtlSetting::Forever,
    TtlSetting::Never,
    "thread-local-random",
  )
}

// The production constructor reads the host's `/etc/resolv.conf`, the file the
// original's `resolvConf` and the JDK's `ResolverConfigurationImpl` both open.
#[test]
fn new_reads_the_etc_resolv_conf() {
  assert_eq!(SystemHostDnsConfiguration::new(), SystemHostDnsConfiguration::reading("/etc/resolv.conf"));
  assert_eq!(SystemHostDnsConfiguration::default(), SystemHostDnsConfiguration::new());
}

// A `nameserver` line without a port gets the original's fallback port 53.
#[test]
fn an_ipv4_name_server_without_a_port_uses_port_53() {
  let (_dir, host) = host_with("nameserver 192.0.2.1\n");
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:53")]));
}

// The JDK brackets an IPv6 literal written without brackets, and the URI then
// has no port, so it is port 53 too.
#[test]
fn an_unbracketed_ipv6_name_server_uses_port_53() {
  let (_dir, host) = host_with("nameserver 2001:db8::1\n");
  assert_eq!(host.name_servers(), Some(vec![addr("[2001:db8::1]:53")]));
}

// A port written after the address is kept, as the `dns://` URI's port.
#[test]
fn a_name_server_port_is_kept() {
  let (_dir, host) = host_with("nameserver 192.0.2.1:5353\nnameserver [2001:db8::1]:5354\n");
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:5353"), addr("[2001:db8::1]:5354")]));
}

// The name servers keep the file's order.
#[test]
fn name_servers_keep_the_file_order() {
  let (_dir, host) = host_with("nameserver 192.0.2.2\nsearch example.com\nnameserver 192.0.2.1\n");
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.2:53"), addr("192.0.2.1:53")]));
}

// The JDK reads one value per `nameserver` line and stops the line at a value
// that starts with `#` or `;`.
#[test]
fn only_the_first_value_of_a_line_is_read() {
  let (_dir, host) = host_with("nameserver 192.0.2.1 192.0.2.2\nnameserver\t#192.0.2.3\nnameserver 192.0.2.4\n");
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:53"), addr("192.0.2.4:53")]));
}

// The JDK reads at most five `nameserver` lines, counting a line whose value
// is a comment.
#[test]
fn at_most_five_name_server_lines_are_read() {
  let (_dir, host) = host_with(
    "nameserver 192.0.2.1\nnameserver ;comment\nnameserver 192.0.2.3\nnameserver 192.0.2.4\nnameserver \
     192.0.2.5\nnameserver 192.0.2.6\n",
  );
  assert_eq!(
    host.name_servers(),
    Some(vec![addr("192.0.2.1:53"), addr("192.0.2.3:53"), addr("192.0.2.4:53"), addr("192.0.2.5:53")])
  );
}

// The keyword must start the line and be followed by a space or a tab.
#[test]
fn lines_that_do_not_start_with_the_keyword_are_skipped() {
  let (_dir, host) = host_with(
    "# nameserver 192.0.2.1\n; nameserver 192.0.2.2\n nameserver 192.0.2.3\nnameservers 192.0.2.4\nnameserver \
     192.0.2.5\n",
  );
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.5:53")]));
}

// The original drops each address it cannot turn into a socket address. This
// port also drops the ones the kernel parser does not accept: host names and
// IPv6 zone indexes. An unbracketed IPv6 literal with dots is not a valid URI
// authority, so the original drops it too.
#[test]
fn unusable_name_servers_are_dropped() {
  let (_dir, host) = host_with(
    "nameserver dns.example\nnameserver fe80::1%en0\nnameserver ::ffff:192.0.2.9\nnameserver 192.0.2.1:99999\nnameserver \
     192.0.2.1\n",
  );
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:53")]));
}

// A `:` with no port after it leaves the URI's port undefined, which is 53.
#[test]
fn an_empty_port_uses_port_53() {
  let (_dir, host) = host_with("nameserver 192.0.2.1:\n");
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:53")]));
}

// Windows and `\r` line ends separate lines, as `BufferedReader.readLine` does.
#[test]
fn carriage_returns_end_lines() {
  let (_dir, host) = host_with("nameserver 192.0.2.1\r\nnameserver 192.0.2.2\rnameserver 192.0.2.3");
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:53"), addr("192.0.2.2:53"), addr("192.0.2.3:53")]));
}

// No usable name server is `None`: the original then fails with
// "Unable to obtain default nameservers".
#[test]
fn a_file_without_name_servers_has_none() {
  let (_dir, host) = host_with("search example.com\n");
  assert_eq!(host.name_servers(), None);
  let (_dir, empty) = host_with("");
  assert_eq!(empty.name_servers(), None);
  let (_dir, unusable) = host_with("nameserver dns.example\n");
  assert_eq!(unusable.name_servers(), None);
}

// The JDK decodes the file leniently, so a stray invalid byte does not hide
// the name servers.
#[test]
fn invalid_utf8_does_not_hide_name_servers() {
  let dir = TempDir::new();
  let host = SystemHostDnsConfiguration::reading(dir.file(b"# \xff\nnameserver 192.0.2.1\n"));
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:53")]));
}

// The search list and `ndots` come from the kernel's `ResolvConfParser`.
#[test]
fn the_search_list_and_ndots_are_read() {
  let (_dir, host) = host_with("nameserver 192.0.2.1\nsearch a.example b.example\noptions ndots:3\n");
  assert_eq!(host.resolv_conf(), Some(ResolvConf::new(strings(&["a.example", "b.example"]), 3)));
}

#[test]
fn a_domain_line_is_the_search_list() {
  let (_dir, host) = host_with("domain corp.example\n");
  assert_eq!(host.resolv_conf(), Some(ResolvConf::new(strings(&["corp.example"]), 1)));
}

// An existing file with nothing to read is still a `resolv.conf`: an empty
// search list and `ndots` 1.
#[test]
fn an_empty_file_is_an_empty_resolv_conf() {
  let (_dir, host) = host_with("");
  assert_eq!(host.resolv_conf(), Some(ResolvConf::new(Vec::new(), 1)));
}

#[test]
fn carriage_returns_end_resolv_conf_lines() {
  let (_dir, host) = host_with("search a.example\r\noptions ndots:2\rdomain b.example");
  assert_eq!(host.resolv_conf(), Some(ResolvConf::new(strings(&["b.example"]), 2)));
}

// A parse failure is `None`, as the original's `Failure` branch.
#[test]
fn an_invalid_ndots_is_none() {
  let (_dir, host) = host_with("options ndots:many\n");
  assert_eq!(host.resolv_conf(), None);
}

// `Files.lines` decodes strictly, so invalid UTF-8 is a parse failure.
#[test]
fn invalid_utf8_is_no_resolv_conf() {
  let dir = TempDir::new();
  let host = SystemHostDnsConfiguration::reading(dir.file(b"search \xff.example\n"));
  assert_eq!(host.resolv_conf(), None);
}

#[test]
fn a_missing_file_has_neither_name_servers_nor_resolv_conf() {
  let dir = TempDir::new();
  let host = SystemHostDnsConfiguration::reading(dir.path().join("missing"));
  assert_eq!(host.name_servers(), None);
  assert_eq!(host.resolv_conf(), None);
}

// A path that exists but cannot be read as a file, such as a directory.
#[test]
fn an_unreadable_path_has_neither_name_servers_nor_resolv_conf() {
  let dir = TempDir::new();
  let host = SystemHostDnsConfiguration::reading(dir.path());
  assert_eq!(host.name_servers(), None);
  assert_eq!(host.resolv_conf(), None);
}

// The file is read again on each call, so a change shows up on the next call.
#[test]
fn each_call_reads_the_file_again() {
  let dir = TempDir::new();
  let host = SystemHostDnsConfiguration::reading(dir.file(b"nameserver 192.0.2.1\n"));
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:53")]));
  dir.file(b"nameserver 192.0.2.2\n");
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.2:53")]));
}

// The `"default"` settings take the host's values through the kernel port.
#[test]
fn default_settings_take_the_values_from_the_file() {
  let (_dir, host) = host_with("nameserver 192.0.2.1\nnameserver 2001:db8::1\nsearch example.com\noptions ndots:2\n");
  let values = DnsSettingsValues::new(&default_config(), &host).expect("the host has name servers");
  assert_eq!(values.name_servers(), &[addr("192.0.2.1:53"), addr("[2001:db8::1]:53")]);
  assert_eq!(values.search_domains(), strings(&["example.com"]).as_slice());
  assert_eq!(values.ndots(), 2);
}

// Without a file the default name servers cannot be determined, the original's
// `IllegalStateException`.
#[test]
fn default_name_servers_fail_without_a_file() {
  let dir = TempDir::new();
  let host = SystemHostDnsConfiguration::reading(dir.path().join("missing"));
  assert_eq!(
    DnsSettingsValues::new(&default_config(), &host),
    Err(DnsSettingsError::UnableToDetermineDefaultNameservers)
  );
}

// An unparseable `resolv.conf` leaves the search list empty and `ndots` at 1.
#[test]
fn an_unparseable_resolv_conf_falls_back_to_the_original_defaults() {
  let (_dir, host) = host_with("nameserver 192.0.2.1\nsearch example.com\noptions ndots:x\n");
  let values = DnsSettingsValues::new(&default_config(), &host).expect("the host has name servers");
  assert!(values.search_domains().is_empty());
  assert_eq!(values.ndots(), 1);
}
