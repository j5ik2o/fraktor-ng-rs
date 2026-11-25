// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 9dfca01566 #3348; the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/AddressSpec.scala
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::address::Address;

// "Address ordering" must { "work correctly with multiple mixed protocols" in { ... } }
#[test]
fn work_correctly_with_multiple_mixed_protocols() {
  let pekko_address1 = Address::with_host_port("pekko", "system", "host1", 1000);
  let pekko_address2 = Address::with_host_port("pekko", "system", "host2", 1000);
  let akka_address1 = Address::with_host_port("akka", "system", "host1", 1000);
  let akka_address2 = Address::with_host_port("akka", "system", "host2", 1000);

  let mut addresses =
    vec![pekko_address1.clone(), pekko_address2.clone(), akka_address1.clone(), akka_address2.clone()];
  addresses.sort();

  assert_eq!(addresses, vec![akka_address1, akka_address2, pekko_address1, pekko_address2]);
}
