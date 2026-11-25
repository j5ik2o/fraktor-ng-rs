// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by ec27d6dd8c #2724, 00b0a99278 #2081; the cases and their inputs are
// unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
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
