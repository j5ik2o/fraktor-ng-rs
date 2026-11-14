//! Tests for the address element primitives and [`Address`](super::Address).

use crate::kernel::address::address_from_uri_string::AddressFromUriString;

use super::Address;
use super::HostPort;
use super::Protocol;
use super::SystemName;

fn protocol(text: &str) -> Protocol {
  match Protocol::new(text) {
    | Ok(protocol) => protocol,
    | Err(message) => panic!("a valid protocol was rejected: {message}"),
  }
}

fn system_name(text: &str) -> SystemName {
  match SystemName::new(text) {
    | Ok(system) => system,
    | Err(message) => panic!("a valid system name was rejected: {message}"),
  }
}

#[test]
fn protocol_rejects_an_empty_string() {
  assert!(Protocol::new("").is_err(), "an empty protocol was accepted");
}

#[test]
fn protocol_keeps_a_non_empty_string() {
  assert_eq!(protocol("pekko").as_str(), "pekko");
}

#[test]
fn protocol_parses_every_non_empty_string() {
  assert_eq!(Protocol::parse("akka"), Some(protocol("akka")));
  assert_eq!(Protocol::parse(""), None, "an empty protocol parsed successfully");
}

#[test]
fn system_name_rejects_an_empty_string() {
  assert!(SystemName::new("").is_err(), "an empty system name was accepted");
}

#[test]
fn system_name_keeps_a_non_empty_string() {
  assert_eq!(system_name("mysys").as_str(), "mysys");
}

#[test]
fn system_name_parses_every_non_empty_string() {
  assert_eq!(SystemName::parse("mysys"), Some(system_name("mysys")));
  assert_eq!(SystemName::parse(""), None, "an empty system name parsed successfully");
}

#[test]
fn host_port_keeps_the_host_and_port() {
  let host_port = HostPort::new("localhost", 2552);

  assert_eq!(host_port.host(), "localhost");
  assert_eq!(host_port.port(), 2552);
}

#[test]
fn host_port_parses_a_host_and_port() {
  assert_eq!(HostPort::parse("localhost", 2552), Some(HostPort::new("localhost", 2552)));
}

#[test]
fn address_new_rejects_an_empty_protocol() {
  assert!(Address::try_new("", "mysys").is_err(), "an empty protocol was accepted");
}

#[test]
fn address_new_rejects_an_empty_system() {
  assert!(Address::try_new("pekko", "").is_err(), "an empty system name was accepted");
}

#[test]
fn address_new_reports_the_violated_constraint() {
  let error = Address::try_new("", "").unwrap_err();

  assert!(error.contains("protocol"), "the error does not name the protocol: {error}");
}

#[test]
fn address_new_builds_a_local_address_from_valid_strings() {
  let address = Address::new("pekko", "mysys");

  assert_eq!(address.protocol(), "pekko");
  assert_eq!(address.system(), "mysys");
  assert_eq!(address.host(), None);
  assert_eq!(address.port(), None);
}

#[test]
fn address_with_host_port_builds_a_remote_address_from_valid_strings() {
  let address = Address::with_host_port("pekko", "mysys", "localhost", 2552);

  assert_eq!(address.host(), Some("localhost"));
  assert_eq!(address.port(), Some(2552));
}

#[test]
fn address_from_parts_builds_from_valid_primitives() {
  let host_port = HostPort::new("host", 1234);

  let address = Address::from_parts(protocol("pekko"), system_name("mysys"), Some(host_port));

  assert_eq!(address.protocol(), "pekko");
  assert_eq!(address.system(), "mysys");
  assert_eq!(address.host(), Some("host"));
  assert_eq!(address.port(), Some(1234));
}

#[test]
fn address_from_parts_without_a_host_port_has_no_port() {
  let address = Address::from_parts(protocol("pekko"), system_name("mysys"), None);

  assert_eq!(address.host(), None);
  assert_eq!(address.port(), None);
}

#[test]
fn address_keeps_its_display_form() {
  assert_eq!(Address::new("pekko", "mysys").to_string(), "pekko://mysys");
  assert_eq!(Address::with_host_port("pekko", "mysys", "localhost", 2552).to_string(), "pekko://mysys@localhost:2552");
}

#[test]
fn addresses_compare_and_hash_by_their_elements() {
  use std::collections::hash_map::DefaultHasher;
  use std::hash::Hash;
  use std::hash::Hasher;

  let equal = Address::with_host_port("pekko", "mysys", "localhost", 2552);
  let host_differs = Address::with_host_port("pekko", "mysys", "otherhost", 2552);
  let port_differs = Address::with_host_port("pekko", "mysys", "localhost", 2551);

  assert_eq!(equal, Address::with_host_port("pekko", "mysys", "localhost", 2552));
  assert_ne!(equal, host_differs);
  assert_ne!(equal, port_differs);

  let mut left = DefaultHasher::new();
  equal.hash(&mut left);
  let mut right = DefaultHasher::new();
  Address::with_host_port("pekko", "mysys", "localhost", 2552).hash(&mut right);
  assert_eq!(left.finish(), right.finish());
}

#[test]
fn from_uri_string_keeps_its_results() {
  assert_eq!(
    AddressFromUriString::apply("pekko://my_sys@host:1234"),
    Address::with_host_port("pekko", "my_sys", "host", 1234)
  );
  assert!(AddressFromUriString::try_apply("pekko://sys@in_valid:5001").is_none());
}

#[test]
fn from_uri_string_rejects_an_empty_system() {
  // An empty system was silently accepted before this change; the
  // domain-primitive rule makes an empty system an invalid value, so the
  // URI is now rejected instead of producing an address with an empty system.
  assert!(AddressFromUriString::try_apply("pekko://@host:1234").is_none());
}
