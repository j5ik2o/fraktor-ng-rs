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
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A network adapter as the Windows IP Helper reports it.

use alloc::{format, string::String, vec::Vec};
use core::net::SocketAddr;

use super::system_host_dns_configuration::server_address;

#[cfg(test)]
#[path = "windows_adapter_test.rs"]
mod tests;

// `ResolverConfigurationImpl.c` joins the name servers into a buffer of
// `MAX_STR_LEN` bytes, its terminating NUL included.
const MAX_STR_LEN: usize = 1024;

// One adapter of the list `GetAdaptersAddresses` returns: whether its
// `OperStatus` is `IfOperStatusUp`, and its DNS server addresses in order. An
// address of a family other than IPv4 and IPv6 is left out, as
// `WSAAddressToStringA` fails on it and the JDK skips it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WindowsAdapter {
  up: bool,
  dns_servers: Vec<SocketAddr>,
}

impl WindowsAdapter {
  // Only the IP Helper on Windows, and the tests, build adapters.
  #[cfg(any(windows, test))]
  pub(crate) fn new(up: bool, dns_servers: Vec<SocketAddr>) -> Self {
    Self { up, dns_servers }
  }

  // The name servers the original gets on Windows from the `adapters`, or
  // `None` when there are none or the adapters could not be read.
  //
  // The JDK's Windows `ResolverConfigurationImpl` takes, from each adapter that
  // is up, each DNS server without an IPv6 scope ID, as `WSAAddressToStringA`
  // writes it, and joins them with commas while they fit `MAX_STR_LEN`
  // (`loadConfig` and `strappend`). `addressesToList` then brackets an IPv6
  // address and keeps the first of equal entries. Each entry becomes a
  // `dns://` URI, read as on Unix.
  pub(crate) fn name_servers(adapters: Option<&[Self]>) -> Option<Vec<SocketAddr>> {
    let mut joined_length = 0;
    let mut entries: Vec<String> = Vec::new();
    for server in adapters?.iter().filter(|adapter| adapter.up).flat_map(|adapter| &adapter.dns_servers) {
      if matches!(server, SocketAddr::V6(server) if server.scope_id() != 0) {
        continue;
      }
      let text = address_text(server);
      let separator = usize::from(joined_length > 0);
      if joined_length + separator + text.len() + 1 > MAX_STR_LEN {
        continue;
      }
      joined_length += separator + text.len();
      let entry = bracket_unless_bsd_style(text);
      if !entries.contains(&entry) {
        entries.push(entry);
      }
    }
    let servers: Vec<SocketAddr> = entries.into_iter().filter_map(server_address).collect();
    (!servers.is_empty()).then_some(servers)
  }
}

// `WSAAddressToStringA` writes the port only when it is not 0, and then
// brackets an IPv6 address.
fn address_text(server: &SocketAddr) -> String {
  if server.port() == 0 {
    format!("{}", server.ip())
  } else {
    format!("{server}")
  }
}

// `addressesToList` brackets an entry with a `:` that does not start with `[`.
fn bracket_unless_bsd_style(text: String) -> String {
  if text.contains(':') && !text.starts_with('[') {
    format!("[{text}]")
  } else {
    text
  }
}
