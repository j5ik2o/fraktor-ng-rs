// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by fef71efd85 #3539, 8353a4f16c #3301; the cases and their inputs are
// unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/RelativeActorPathSpec.scala
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::address::RelativeActorPath;

fn elements(path: &str) -> Vec<String> {
  RelativeActorPath::try_apply(path).unwrap_or_default()
}

// "RelativeActorPath" must { "match single name" in { ... } }
#[test]
fn match_single_name() {
  assert_eq!(elements("foo"), vec!["foo".to_string()]);
}

// "RelativeActorPath" must { "match path separated names" in { ... } }
#[test]
fn match_path_separated_names() {
  assert_eq!(elements("foo/bar/baz"), vec!["foo".to_string(), "bar".to_string(), "baz".to_string()]);
}

// "RelativeActorPath" must { "match url encoded name" in { ... } }
#[test]
fn match_url_encoded_name() {
  // java.net.URLEncoder.encode("pekko://ClusterSystem@127.0.0.1:7355", "UTF-8")
  let name = "pekko%3A%2F%2FClusterSystem%40127.0.0.1%3A7355";
  assert_eq!(elements(name), vec![name.to_string()]);
}

// "RelativeActorPath" must { "match path with uid fragment" in { ... } }
#[test]
fn match_path_with_uid_fragment() {
  assert_eq!(elements("foo/bar/baz#1234"), vec!["foo".to_string(), "bar".to_string(), "baz#1234".to_string()]);
}
