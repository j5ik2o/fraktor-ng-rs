// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/WildcardIndexSpec.scala
//
// The original's eight cases under "wildcard index must" are kept as one test
// each, named after the case, with the original name as a comment. Two
// behaviours differ from the original on purpose (see the design document for
// #187, section 2.2): the key a suffix child is looked up by, and whether a
// root holding only suffix children counts as empty. Neither difference is
// exercised by these eight cases; supplementary regression tests in the
// kernel pin them.

use fraktor_actor::kernel::wildcard_index::{WildcardIndex, WildcardIndexError};

fn empty_index() -> WildcardIndex<i32> {
  WildcardIndex::new()
}

fn insert(index: &WildcardIndex<i32>, elems: &[&str], value: i32) -> WildcardIndex<i32> {
  index.insert(elems, value).expect("the spec inserts no double wildcard suffix")
}

fn find(index: &WildcardIndex<i32>, elems: &[&str]) -> Option<i32> {
  index.find(elems).copied()
}

// "allow to insert elements using Arrays of strings"
#[test]
fn allow_to_insert_elements_using_arrays_of_strings() {
  let _: WildcardIndex<i32> = insert(&empty_index(), &["a", "b"], 1);
  let _: WildcardIndex<i32> = insert(&empty_index(), &["a"], 1);
  let _: WildcardIndex<i32> = insert(&empty_index(), &[], 1);
}

// "allow to find inserted elements"
#[test]
fn allow_to_find_inserted_elements() {
  let tree = insert(&insert(&insert(&empty_index(), &["a"], 1), &["a", "b"], 2), &["a", "c"], 3);
  assert_eq!(find(&tree, &["a", "b"]), Some(2));
  assert_eq!(find(&tree, &["a"]), Some(1));
  assert_eq!(find(&tree, &["x"]), None);
  assert_eq!(find(&tree, &[]), None);
}

// "match all elements in the subArray when it contains a wildcard"
#[test]
fn match_all_elements_in_the_sub_array_when_it_contains_a_wildcard() {
  let tree1 = insert(&insert(&empty_index(), &["a"], 1), &["a", "*"], 1);
  assert_eq!(find(&tree1, &["z"]), None);
  assert_eq!(find(&tree1, &["a"]), Some(1));
  assert_eq!(find(&tree1, &["a", "b"]), Some(1));
  assert_eq!(find(&tree1, &["a", "x"]), Some(1));

  let tree2 = insert(&insert(&empty_index(), &["a", "*"], 1), &["a", "*", "c"], 2);
  assert_eq!(find(&tree2, &["z"]), None);
  assert_eq!(find(&tree2, &["a", "b"]), Some(1));
  assert_eq!(find(&tree2, &["a", "x"]), Some(1));
  assert_eq!(find(&tree2, &["a", "x", "c"]), Some(2));
  assert_eq!(find(&tree2, &["a", "x", "y"]), None);
}

// "match all elements in the subArray when it contains a wildcard suffix"
#[test]
fn match_all_elements_in_the_sub_array_when_it_contains_a_wildcard_suffix() {
  let tree1 = insert(&insert(&empty_index(), &["a*"], 1), &["b", "c*"], 2);
  assert_eq!(find(&tree1, &["z"]), None);
  assert_eq!(find(&tree1, &["a"]), Some(1));
  assert_eq!(find(&tree1, &["aa"]), Some(1));
  assert_eq!(find(&tree1, &["aa", "b"]), None);
  assert_eq!(find(&tree1, &["b", "c"]), Some(2));
  assert_eq!(find(&tree1, &["b", "cc"]), Some(2));
  assert_eq!(find(&tree1, &["b", "x"]), None);

  let tree2 = insert(&insert(&empty_index(), &["a", "b*"], 1), &["b", "c*", "d"], 2);
  assert_eq!(find(&tree2, &["z"]), None);
  assert_eq!(find(&tree2, &["a", "b"]), Some(1));
  assert_eq!(find(&tree2, &["a", "bb"]), Some(1));
  assert_eq!(find(&tree2, &["a", "c"]), None);
  assert_eq!(find(&tree2, &["b", "c", "d"]), Some(2));
  assert_eq!(find(&tree2, &["b", "cc", "d"]), Some(2));
  assert_eq!(find(&tree2, &["b", "c", "x"]), None);
  assert_eq!(find(&tree2, &["b", "cc", "x"]), None);
}

// "fail when a double wildcard is used as a suffix"
#[test]
fn fail_when_a_double_wildcard_is_used_as_a_suffix() {
  // The original expects an IllegalArgumentException; the port returns an
  // error value instead (see the design document for #187, section 2.2).
  assert_eq!(
    empty_index().insert(&["a**"], 1),
    Err(WildcardIndexError::DoubleWildcardAsSuffix { element: "a**".to_string() })
  );
}

// "never find anything when emptyIndex"
#[test]
fn never_find_anything_when_empty() {
  assert_eq!(find(&empty_index(), &["a"]), None);
  assert_eq!(find(&empty_index(), &["a", "b"]), None);
  assert_eq!(find(&empty_index(), &[]), None);
}

// "match all remaining elements when it contains a terminal double wildcard"
#[test]
fn match_all_remaining_elements_when_it_contains_a_terminal_double_wildcard() {
  let tree1 = insert(&empty_index(), &["a", "**"], 1);
  assert_eq!(find(&tree1, &["z"]), None);
  assert_eq!(find(&tree1, &["a", "b"]), Some(1));
  assert_eq!(find(&tree1, &["a", "x"]), Some(1));
  assert_eq!(find(&tree1, &["a", "x", "y"]), Some(1));

  let tree2 = insert(&empty_index(), &["**"], 1);
  assert_eq!(find(&tree2, &["anything", "I", "want"]), Some(1));
  assert_eq!(find(&tree2, &["anything"]), Some(1));
}

// "ignore non-terminal double wildcards"
#[test]
fn ignore_non_terminal_double_wildcards() {
  let tree = insert(&empty_index(), &["a", "**", "c"], 1);
  assert_eq!(find(&tree, &["a", "x", "y", "c"]), None);
  assert_eq!(find(&tree, &["a", "x", "y"]), None);
}
