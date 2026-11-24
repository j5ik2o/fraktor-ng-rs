//! Unit tests for [`ResolvConf`](super::ResolvConf).

use alloc::{string::String, vec};

use super::ResolvConf;

// The accessors expose the values the parser built, in order.
#[test]
fn the_accessors_return_the_stored_search_list_and_ndots() {
  let conf = ResolvConf::new(vec![String::from("example.com"), String::from("example.net")], 3);
  assert_eq!(conf.search(), [String::from("example.com"), String::from("example.net")]);
  assert_eq!(conf.ndots(), 3);
}
