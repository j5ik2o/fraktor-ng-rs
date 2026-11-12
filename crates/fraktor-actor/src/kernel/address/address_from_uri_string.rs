//! Parses addresses from their `protocol://system@host:port` string form.

use crate::kernel::address::address_value::Address;
use crate::kernel::address::uri_parts::UriParts;

/// Entry point for parsing an [`Address`] from a URI string.
#[derive(Clone, Copy, Debug, Default)]
pub struct AddressFromUriString;

impl AddressFromUriString {
  /// Parses an address, returning `None` when the string is not a valid address URI.
  pub fn try_apply(addr: &str) -> Option<Address> {
    UriParts::parse(addr).and_then(|parts| Address::from_uri_parts(&parts))
  }

  /// Parses an address.
  ///
  /// # Panics
  ///
  /// Panics when `addr` is not a valid address URI.
  pub fn apply(addr: &str) -> Address {
    match Self::try_apply(addr) {
      | Some(address) => address,
      | None => panic!("Malformed URL: {}", addr),
    }
  }
}
