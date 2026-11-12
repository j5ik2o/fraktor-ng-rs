//! Immutable value describing where an actor system listens.

use core::fmt;

use crate::kernel::address::uri_parts::UriParts;

/// The protocol, system name, and optional host and port of an actor system.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Address {
  protocol: String,
  system: String,
  host: Option<String>,
  port: Option<u16>,
}

impl Address {
  /// Creates a local address without host or port.
  pub fn new(protocol: &str, system: &str) -> Self {
    Self::from_parts(protocol, system, None, None)
  }

  /// Creates an address with a host and port.
  pub fn with_host_port(protocol: &str, system: &str, host: &str, port: u16) -> Self {
    Self::from_parts(protocol, system, Some(host.to_string()), Some(port))
  }

  pub(crate) fn from_parts(protocol: &str, system: &str, host: Option<String>, port: Option<u16>) -> Self {
    Self { protocol: protocol.to_string(), system: system.to_string(), host, port }
  }

  /// Returns the protocol, for example `pekko`.
  pub fn protocol(&self) -> &str {
    &self.protocol
  }

  /// Returns the actor system name.
  pub fn system(&self) -> &str {
    &self.system
  }

  /// Returns the host when the address is reachable from other nodes.
  pub fn host(&self) -> Option<&str> {
    self.host.as_deref()
  }

  /// Returns the port, which is present only together with a host.
  pub fn port(&self) -> Option<u16> {
    self.port
  }

  /// Reports whether the host contains characters that a URI would reject.
  pub fn has_invalid_host_characters(&self) -> bool {
    self.host.as_deref().is_some_and(|host| host.rsplit('.').next().is_some_and(|last| last.contains('_')))
  }

  /// Returns an error when the host contains characters a URI would reject.
  pub fn check_host_characters(&self) -> Result<(), String> {
    if self.has_invalid_host_characters() {
      Err(format!("Using invalid host characters '{:?}' in the Address is not allowed.", self.host))
    } else {
      Ok(())
    }
  }

  pub(crate) fn from_uri_parts(parts: &UriParts<'_>) -> Option<Self> {
    match (parts.user_info(), parts.port()) {
      | (None, None) => Some(Self::from_parts(parts.scheme(), parts.host(), None, None)),
      | (None, Some(_)) => None,
      | (Some(system), Some(port)) => {
        Some(Self::from_parts(parts.scheme(), system, Some(parts.host().to_string()), Some(port)))
      },
      | (Some(_), None) => None,
    }
  }
}

impl fmt::Display for Address {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}://{}", self.protocol, self.system)?;
    if let Some(host) = &self.host {
      write!(formatter, "@{}", host)?;
    }
    if let Some(port) = self.port {
      write!(formatter, ":{}", port)?;
    }
    Ok(())
  }
}
