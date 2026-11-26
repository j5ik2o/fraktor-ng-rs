// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright 2017 The Netty Project
//
// The Netty Project licenses this file to you under the Apache License,
// version 2.0 (the "License"); you may not use this file except in compliance
// with the License. You may obtain a copy of the License at:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

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
  hash::{BuildHasher, Hasher},
  net::{Ipv6Addr, SocketAddr, SocketAddrV6},
  time::Duration,
};
use std::{
  collections::hash_map::RandomState,
  fs::{self, DirBuilder},
  io::{self, BufReader, ErrorKind, Read},
  path::{Path, PathBuf},
};

use super::{read_name_servers, SystemHostDnsConfiguration};
use crate::kernel::io::dns::{
  DnsSettingsConfig, DnsSettingsError, DnsSettingsValues, HostDnsConfiguration, NameserversSetting, NdotsSetting,
  ResolvConf, SearchDomainsSetting, TtlSetting,
};

// A temporary directory this test created, removed when the test ends.
//
// The directory is created exclusively under a fresh random name, so a path
// that already exists, whoever made it, is never reused or removed; on Unix
// only the owner can enter it.
struct TempDir(PathBuf);

impl TempDir {
  fn new() -> Self {
    let base = std::env::temp_dir();
    for _ in 0..16 {
      let name = format!("fraktor-host-dns-{:016x}", RandomState::new().build_hasher().finish());
      let path = base.join(name);
      match private_directory_builder().create(&path) {
        | Ok(()) => return Self(path),
        | Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
        | Err(error) => panic!("create a temporary directory: {error}"),
      }
    }
    panic!("no unused temporary directory name");
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

// `create` (not `create_all`) fails when the path already exists.
fn private_directory_builder() -> DirBuilder {
  let builder = DirBuilder::new();
  #[cfg(unix)]
  let builder = {
    let mut builder = builder;
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder
  };
  builder
}

// The helper never takes over a path that already exists: creating it again
// fails, and each helper directory is new, owner-only, and left alone by
// another helper's cleanup.
#[test]
fn temporary_directories_are_created_exclusively() {
  let first = TempDir::new();
  let second = TempDir::new();
  assert_ne!(first.path(), second.path());
  let error = private_directory_builder().create(first.path()).expect_err("the directory already exists");
  assert_eq!(error.kind(), ErrorKind::AlreadyExists);
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(first.path()).expect("the directory exists").permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
  }
  let first_path = first.path().to_path_buf();
  drop(first);
  assert!(!first_path.exists());
  assert!(second.path().is_dir());
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

// Outside Windows the production constructor reads the host's
// `/etc/resolv.conf`, the file the original's `resolvConf` and the JDK's
// `ResolverConfigurationImpl` both open.
#[test]
fn a_resolv_conf_host_reads_the_etc_resolv_conf() {
  assert_eq!(SystemHostDnsConfiguration::for_platform(false), SystemHostDnsConfiguration::reading("/etc/resolv.conf"));
}

// On Windows the name servers come from the IP Helper, as the JDK's Windows
// `ResolverConfigurationImpl` reads them, and the original skips `resolv.conf`.
#[test]
fn a_windows_host_reads_the_ip_helper_and_has_no_resolv_conf() {
  let host = SystemHostDnsConfiguration::for_platform(true);
  assert_eq!(host, SystemHostDnsConfiguration::ip_helper());
  assert_eq!(host.resolv_conf(), None);
}

#[test]
fn new_follows_the_platform_it_runs_on() {
  assert_eq!(SystemHostDnsConfiguration::new(), Ok(SystemHostDnsConfiguration::for_platform(cfg!(windows))));
}

// On a real Windows host the IP Helper answers, and a name server it yields
// has no IPv6 scope ID, which the JDK skips. The host's own servers are not
// compared, as they depend on its network.
#[cfg(windows)]
#[test]
fn the_ip_helper_of_a_windows_host_is_read() {
  assert!(crate::adaptor::windows_ip_helper::read_adapters().is_some());
  let host = SystemHostDnsConfiguration::new().expect("Windows is supported");
  for server in host.name_servers().unwrap_or_default() {
    if let SocketAddr::V6(server) = server {
      assert_eq!(server.scope_id(), 0);
    }
  }
}

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
// IPv6 zones named by interface. An unbracketed IPv6 literal with dots is not a valid URI
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

// A full-width `ndots` is a decimal digit to the original's `toInt`
// (`Integer.parseInt`), so the search list and `ndots` are both read.
#[test]
fn a_full_width_ndots_is_read_with_the_search_list() {
  let (_dir, host) = host_with("search a.example b.example\noptions ndots:\u{FF15}\n");
  assert_eq!(host.resolv_conf(), Some(ResolvConf::new(strings(&["a.example", "b.example"]), 5)));
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

// The `"default"` settings take a full-width `ndots` and the search list from
// the file, instead of falling back to the defaults.
#[test]
fn default_settings_take_a_full_width_ndots_from_the_file() {
  let (_dir, host) = host_with("nameserver 192.0.2.1\nsearch example.com\noptions ndots:\u{0663}\n");
  let values = DnsSettingsValues::new(&default_config(), &host).expect("the host has name servers");
  assert_eq!(values.search_domains(), strings(&["example.com"]).as_slice());
  assert_eq!(values.ndots(), 3);
}

// A decimal digit outside the BMP is not a digit to `Integer.parseInt`, so the
// file is unparseable and the search list and `ndots` fall back to the
// original's defaults, while the name servers are still read.
#[test]
fn a_supplementary_digit_ndots_falls_back_to_the_original_defaults() {
  let (_dir, host) = host_with("nameserver 192.0.2.1\nsearch example.com\noptions ndots:\u{1D7D3}\n");
  assert_eq!(host.resolv_conf(), None);
  let values = DnsSettingsValues::new(&default_config(), &host).expect("the host has name servers");
  assert_eq!(values.name_servers(), &[addr("192.0.2.1:53")]);
  assert!(values.search_domains().is_empty());
  assert_eq!(values.ndots(), 1);
}

// The JDK keeps `192.0.2.1#local` as one value, and the `dns://` URI ends its
// authority at `#`, `?` or `/`, so the original reads the address before it.
// An unbracketed IPv6 literal gets its brackets around the `#` too, which the
// URI cannot read, so it is dropped.
#[test]
fn a_name_server_ends_where_the_uri_authority_ends() {
  let (_dir, host) = host_with(
    "nameserver 192.0.2.1#local\nnameserver 192.0.2.2:5353#x\nnameserver [2001:db8::1]#x\nnameserver \
     192.0.2.3/x\nnameserver 192.0.2.4?x\nnameserver 2001:db8::2#x\n",
  );
  assert_eq!(
    host.name_servers(),
    Some(vec![
      addr("192.0.2.1:53"),
      addr("192.0.2.2:5353"),
      addr("[2001:db8::1]:53"),
      addr("192.0.2.3:53"),
      addr("192.0.2.4:53")
    ])
  );
}

// A trailing comment on the only name server still gives the default settings
// a name server.
#[test]
fn default_name_servers_read_an_address_with_a_trailing_comment() {
  let (_dir, host) = host_with("nameserver 192.0.2.1#local\n");
  let values = DnsSettingsValues::new(&default_config(), &host).expect("the host has a name server");
  assert_eq!(values.name_servers(), &[addr("192.0.2.1:53")]);
}

// The original's `ResolvConfParser` has no trailing comments: on a `search`
// line, `#` and the words after it are search domains too. Some resolvers,
// such as FreeBSD's, end the line at `#`; this port keeps the original.
#[test]
fn a_trailing_comment_on_a_search_line_is_kept_as_the_original_does() {
  let (_dir, host) = host_with("search corp.example # managed\n");
  assert_eq!(host.resolv_conf(), Some(ResolvConf::new(strings(&["corp.example", "#", "managed"]), 1)));
}

// A numeric zone index is a valid `dns://` URI host, and the original's
// `InetSocketAddress` keeps it as the scope ID.
#[test]
fn a_numeric_ipv6_scope_id_is_kept() {
  let (_dir, host) = host_with(
    "nameserver fe80::1%2
nameserver [fe80::2%3]:5353
",
  );
  let link_local = |segment: u16, port: u16, scope_id: u32| {
    SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, segment), port, 0, scope_id))
  };
  assert_eq!(host.name_servers(), Some(vec![link_local(1, 53, 2), link_local(2, 5353, 3)]));
}

// A zone that is not a valid URI scope ID, or does not fit a `u32`, is dropped.
#[test]
fn an_invalid_ipv6_scope_id_is_dropped() {
  let (_dir, host) =
    host_with("nameserver fe80::1%\nnameserver fe80::1%2-3\nnameserver fe80::1%99999999999\nnameserver 192.0.2.1\n");
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:53")]));
}

// `new URI("dns://" + server)` throws for characters a URI may not hold, and
// the original then drops the server. A valid path, query or fragment after
// the authority keeps it; this is how a trailing `#comment` is told apart from
// a broken URI.
#[test]
fn a_name_server_that_is_not_a_valid_uri_is_dropped() {
  let (_dir, host) = host_with(
    "nameserver 192.0.2.1#a#b\nnameserver 192.0.2.2#a|b\nnameserver 192.0.2.3?x^y\nnameserver 192.0.2.4/a<b\nnameserver \
     192.0.2.5#%zz\n",
  );
  assert_eq!(host.name_servers(), None);
}

#[test]
fn a_name_server_with_a_valid_uri_tail_is_kept() {
  let (_dir, host) = host_with(
    "nameserver 192.0.2.1#local\nnameserver 192.0.2.2/p;x?q=1#f?/\nnameserver 192.0.2.3#%41\nnameserver \
     192.0.2.4#caf\u{e9}\n",
  );
  assert_eq!(
    host.name_servers(),
    Some(vec![addr("192.0.2.1:53"), addr("192.0.2.2:53"), addr("192.0.2.3:53"), addr("192.0.2.4:53")])
  );
}

// The URI's user information is not part of the host.
#[test]
fn user_information_before_the_host_is_skipped() {
  let (_dir, host) = host_with("nameserver user@192.0.2.1\nnameserver us[er@192.0.2.2\n");
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.1:53")]));
}

// Hands out its bytes, then fails, as a read that breaks part way.
struct FailingAfter(&'static [u8]);

impl Read for FailingAfter {
  fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
    if self.0.is_empty() {
      return Err(io::Error::other("the device went away"));
    }
    let count = buffer.len().min(self.0.len());
    buffer[..count].copy_from_slice(&self.0[..count]);
    self.0 = &self.0[count..];
    Ok(count)
  }
}

// The JDK's `resolvconf` catches the `IOException` after its read loop, so the
// name servers read before the failure are kept.
#[test]
fn name_servers_read_before_an_io_error_are_kept() {
  let reader = BufReader::with_capacity(8, FailingAfter(b"nameserver 192.0.2.1\nnameserver 192.0.2.2\nnameserver 19"));
  assert_eq!(read_name_servers(reader), Some(vec![addr("192.0.2.1:53"), addr("192.0.2.2:53")]));
}

#[test]
fn an_io_error_before_any_name_server_is_none() {
  assert_eq!(read_name_servers(BufReader::new(FailingAfter(b""))), None);
}

// `BufferedReader.readLine` returns a line as soon as it reads its `\r`, so a
// line ended by `\r` is kept even when the next read fails; the unfinished
// line after it is not.
#[test]
fn a_line_ended_by_a_carriage_return_is_kept_before_an_io_error() {
  let reader = BufReader::with_capacity(8, FailingAfter(b"nameserver 192.0.2.1\rnameserver 19"));
  assert_eq!(read_name_servers(reader), Some(vec![addr("192.0.2.1:53")]));
  let reader = BufReader::with_capacity(8, FailingAfter(b"nameserver 192.0.2.1\r\nnameserver 192.0.2.2"));
  assert_eq!(read_name_servers(reader), Some(vec![addr("192.0.2.1:53")]));
}

// The JDK stops a line at a value that starts with `#` or `;`, before the
// value becomes a URI, so `;user@192.0.2.1` is a comment, not user
// information. The line still counts toward the five `nameserver` lines.
#[test]
fn a_value_starting_a_comment_is_not_a_name_server() {
  let (_dir, host) = host_with(
    "nameserver ;user@192.0.2.1\nnameserver #192.0.2.2\nnameserver user@192.0.2.3\nnameserver 192.0.2.4\nnameserver \
     192.0.2.5\nnameserver 192.0.2.6\n",
  );
  assert_eq!(host.name_servers(), Some(vec![addr("192.0.2.3:53"), addr("192.0.2.4:53"), addr("192.0.2.5:53")]));
}
