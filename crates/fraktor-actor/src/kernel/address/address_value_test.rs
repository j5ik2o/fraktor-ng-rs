use core::cmp::Ordering;

use super::Address;

#[test]
fn order_compares_system_before_host_and_host_before_port() {
  let first_system = Address::with_host_port("pekko", "a", "z", 9000);
  let first_host = Address::with_host_port("pekko", "b", "a", 9000);
  let first_port = Address::with_host_port("pekko", "b", "z", 1000);
  let last = Address::with_host_port("pekko", "b", "z", 9000);

  assert!(first_system < first_host);
  assert!(first_host < first_port);
  assert!(first_port < last);
}

#[test]
fn order_distinguishes_a_local_address_from_an_explicit_empty_host_and_zero_port() {
  let local = Address::new("pekko", "system");
  let remote = Address::with_host_port("pekko", "system", "", 0);

  assert_ne!(local, remote);
  assert_eq!(local.cmp(&remote), Ordering::Less);
  assert_eq!(remote.partial_cmp(&local), Some(Ordering::Greater));
}

#[test]
fn order_uses_jvm_string_comparison_for_non_ascii_names() {
  let supplementary = Address::new("pekko", "\u{10000}");
  let basic_plane = Address::new("pekko", "\u{e000}");

  assert!(supplementary < basic_plane);
}
