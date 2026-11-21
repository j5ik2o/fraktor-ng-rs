// Ported from Pekko v1.7.1:
//   actor-tests/src/test/java/org/apache/pekko/actor/AddressTest.java
//
// Original test name is kept as a comment next to the ported case.

use fraktor_actor::kernel::address::Address;

// "portAddressAccessible"
#[test]
fn port_address_accessible() {
  let address = Address::with_host_port("pekko", "MySystem", "localhost", 2525);
  assert_eq!(address.port(), Some(2525));
  assert_eq!(address.host(), Some("localhost"));
}
