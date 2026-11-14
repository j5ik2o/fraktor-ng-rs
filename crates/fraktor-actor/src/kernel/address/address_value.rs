//! Immutable value describing where an actor system listens.

use core::fmt;

use crate::kernel::address::uri_parts::UriParts;

#[cfg(test)]
#[path = "address_value_test.rs"]
mod tests;

/// The protocol of an address, for example `pekko`, which must not be empty.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Protocol(String);

impl Protocol {
  /// Creates a protocol, or returns an error when `text` is empty.
  pub(crate) fn new(text: &str) -> Result<Self, String> {
    if text.is_empty() {
      Err("the protocol of an address must not be empty".to_string())
    } else {
      Ok(Self(text.to_string()))
    }
  }

  /// Parses a protocol, returning `None` when `text` is empty.
  pub(crate) fn parse(text: &str) -> Option<Self> {
    Self::new(text).ok()
  }

  /// Returns the protocol text.
  pub(crate) fn as_str(&self) -> &str {
    &self.0
  }
}

/// The name of an actor system within an address, which must not be empty.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct SystemName(String);

impl SystemName {
  /// Creates a system name, or returns an error when `text` is empty.
  pub(crate) fn new(text: &str) -> Result<Self, String> {
    if text.is_empty() {
      Err("the system name of an address must not be empty".to_string())
    } else {
      Ok(Self(text.to_string()))
    }
  }

  /// Parses a system name, returning `None` when `text` is empty.
  pub(crate) fn parse(text: &str) -> Option<Self> {
    Self::new(text).ok()
  }

  /// Returns the system name text.
  pub(crate) fn as_str(&self) -> &str {
    &self.0
  }
}

/// The host and port of a reachable address, which are always present together.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct HostPort {
  host: String,
  port: u16,
}

impl HostPort {
  /// Creates a host and port pair.
  pub(crate) fn new(host: &str, port: u16) -> Self {
    Self { host: host.to_string(), port }
  }

  /// Parses a host and port pair.
  pub(crate) fn parse(host: &str, port: u16) -> Option<Self> {
    Some(Self::new(host, port))
  }

  /// Returns the host.
  pub(crate) fn host(&self) -> &str {
    &self.host
  }

  /// Returns the port.
  pub(crate) fn port(&self) -> u16 {
    self.port
  }

  /// Reports whether the host contains characters that a URI would reject.
  pub(crate) fn has_invalid_characters(&self) -> bool {
    self.host.rsplit('.').next().is_some_and(|last| last.contains('_'))
  }
}

/// The protocol, system name, and optional host and port of an actor system.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Address {
  protocol: Protocol,
  system: SystemName,
  host_port: Option<HostPort>,
}

impl Address {
  /// Creates a local address without host or port.
  ///
  /// # Panics
  ///
  /// Panics when `protocol` or `system` is empty, matching the address the
  /// ported tests build from literals.
  pub fn new(protocol: &str, system: &str) -> Self {
    Self::try_new(protocol, system).unwrap_or_else(|message| panic!("{message}"))
  }

  /// Creates a local address without host or port, or reports the violated
  /// constraint.
  pub fn try_new(protocol: &str, system: &str) -> Result<Self, String> {
    Ok(Self::from_parts(Protocol::new(protocol)?, SystemName::new(system)?, None))
  }

  /// Creates an address with a host and port.
  ///
  /// # Panics
  ///
  /// Panics when `protocol` or `system` is empty.
  pub fn with_host_port(protocol: &str, system: &str, host: &str, port: u16) -> Self {
    Self::try_with_host_port(protocol, system, host, port).unwrap_or_else(|message| panic!("{message}"))
  }

  /// Creates an address with a host and port, or reports the violated
  /// constraint.
  pub fn try_with_host_port(protocol: &str, system: &str, host: &str, port: u16) -> Result<Self, String> {
    Ok(Self::from_parts(Protocol::new(protocol)?, SystemName::new(system)?, Some(HostPort::new(host, port))))
  }

  pub(crate) fn from_parts(protocol: Protocol, system: SystemName, host_port: Option<HostPort>) -> Self {
    Self { protocol, system, host_port }
  }

  /// Returns the protocol, for example `pekko`.
  pub fn protocol(&self) -> &str {
    self.protocol.as_str()
  }

  /// Returns the actor system name.
  pub fn system(&self) -> &str {
    self.system.as_str()
  }

  /// Returns the host when the address is reachable from other nodes.
  pub fn host(&self) -> Option<&str> {
    self.host_port.as_ref().map(HostPort::host)
  }

  /// Returns the port, which is present only together with a host.
  pub fn port(&self) -> Option<u16> {
    self.host_port.as_ref().map(HostPort::port)
  }

  /// Reports whether the host contains characters that a URI would reject.
  pub fn has_invalid_host_characters(&self) -> bool {
    self.host_port.as_ref().is_some_and(HostPort::has_invalid_characters)
  }

  /// Returns an error when the host contains characters a URI would reject.
  pub fn check_host_characters(&self) -> Result<(), String> {
    if self.has_invalid_host_characters() {
      Err(format!("Using invalid host characters '{:?}' in the Address is not allowed.", self.host()))
    } else {
      Ok(())
    }
  }

  pub(crate) fn from_uri_parts(parts: &UriParts<'_>) -> Option<Self> {
    let protocol = Protocol::parse(parts.scheme())?;
    match (parts.user_info(), parts.port()) {
      | (None, None) => Some(Self::from_parts(protocol, SystemName::parse(parts.host())?, None)),
      | (None, Some(_)) => None,
      | (Some(system), Some(port)) => {
        let system = SystemName::parse(system)?;
        Some(Self::from_parts(protocol, system, HostPort::parse(parts.host(), port)))
      },
      | (Some(_), None) => None,
    }
  }
}

impl fmt::Display for Address {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}://{}", self.protocol.as_str(), self.system.as_str())?;
    if let Some(host_port) = &self.host_port {
      write!(formatter, "@{}:{}", host_port.host(), host_port.port())?;
    }
    Ok(())
  }
}
