//! The error a platform without a `resolv.conf` host configuration reports.

use core::fmt;

/// The reason [`SystemHostDnsConfiguration::new`](super::SystemHostDnsConfiguration::new)
/// rejected the host it runs on.
///
/// Windows keeps its name servers in the registry rather than in
/// `/etc/resolv.conf`, and reading them is not ported (issue #160). On such a
/// host, set the `nameservers` setting explicitly with
/// [`NameserversSetting::Single`](crate::kernel::io::dns::NameserversSetting::Single)
/// or [`NameserversSetting::List`](crate::kernel::io::dns::NameserversSetting::List),
/// or pass your own
/// [`HostDnsConfiguration`](crate::kernel::io::dns::HostDnsConfiguration) to
/// [`DnsSettingsValues::new`](crate::kernel::io::dns::DnsSettingsValues::new).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedHostDnsPlatformError(());

impl UnsupportedHostDnsPlatformError {
  pub(crate) fn new() -> Self {
    Self(())
  }
}

impl fmt::Display for UnsupportedHostDnsPlatformError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(
      "reading the host DNS configuration is not supported on this platform; set the name servers explicitly",
    )
  }
}

impl std::error::Error for UnsupportedHostDnsPlatformError {}
