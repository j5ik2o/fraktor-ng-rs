//! The root of the actor hierarchy, carrying the address and the leading name.

use core::fmt;

use crate::kernel::actor_path::actor_path_value::ActorPath;
use crate::kernel::address::Address;

/// The root actor path of an actor system.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RootActorPath {
  address: Address,
  name: String,
}

impl RootActorPath {
  /// Creates a root path for `address` with the default name `/`.
  pub fn new(address: Address) -> Self {
    Self { address, name: "/".to_string() }
  }

  /// Creates a root path with an explicit name.
  ///
  /// # Errors
  ///
  /// Returns an error when `name` contains the path separator `/` after the
  /// first character, or the fragment separator `#`.
  pub fn with_name(address: Address, name: &str) -> Result<Self, String> {
    if !(name.len() == 1 || name.get(1..).is_some_and(|rest| !rest.contains('/'))) {
      return Err(format!(
        "/ may only exist at the beginning of the root actors name, \
                 it is a path separator and is not legal in ActorPath names: [{}]",
        name
      ));
    }
    if name.contains('#') {
      return Err(format!("# is a fragment separator and is not legal in ActorPath names: [{}]", name));
    }
    Ok(Self { address, name: name.to_string() })
  }

  /// Returns the address of this actor system.
  pub fn address(&self) -> &Address {
    &self.address
  }

  /// Returns the root name, which is `/` by default.
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Returns a child path.
  ///
  /// # Panics
  ///
  /// Panics when `child` contains `'/'` or `'#'`.
  pub fn with_child(&self, child: &str) -> ActorPath {
    ActorPath::new(self.clone(), Vec::new()).with_child(child)
  }

  /// Returns the path without its address, which is `/` for the root.
  pub fn to_string_without_address(&self) -> String {
    "/".to_string()
  }

  /// Returns this root path, replacing the address with `address` unless it
  /// already carries a host.
  pub fn to_string_with_address(&self, address: &Address) -> String {
    if self.address.host().is_some() {
      self.to_string()
    } else {
      format!("{}{}", address, self.name)
    }
  }
}

impl fmt::Display for RootActorPath {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}{}", self.address, self.name)
  }
}
