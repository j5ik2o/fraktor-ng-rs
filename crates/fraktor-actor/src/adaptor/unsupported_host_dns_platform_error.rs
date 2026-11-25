//! The error for a platform whose host DNS configuration cannot be read.

use core::fmt;

/// The reason [`SystemHostDnsConfiguration::new`](super::SystemHostDnsConfiguration::new)
/// would reject the host it runs on.
///
/// No platform is rejected any more: Windows, once rejected, now reads its
/// name servers from the IP Helper (issue #160). The type stays so that code
/// matching on the error still compiles. A host whose name servers cannot be
/// read can still set the `nameservers` setting explicitly with
/// [`NameserversSetting::Single`](crate::kernel::io::dns::NameserversSetting::Single)
/// or [`NameserversSetting::List`](crate::kernel::io::dns::NameserversSetting::List),
/// or pass its own
/// [`HostDnsConfiguration`](crate::kernel::io::dns::HostDnsConfiguration) to
/// [`DnsSettingsValues::new`](crate::kernel::io::dns::DnsSettingsValues::new).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedHostDnsPlatformError(());

impl fmt::Display for UnsupportedHostDnsPlatformError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(
      "reading the host DNS configuration is not supported on this platform; set the name servers explicitly",
    )
  }
}

impl std::error::Error for UnsupportedHostDnsPlatformError {}
