use alloc::vec::Vec;

use super::DoubleLinkedList;

fn values(list: &DoubleLinkedList<&'static str>) -> Vec<&'static str> {
  list.forward_iter().map(|node| list[node]).collect()
}

#[test]
fn remove_returns_the_value_and_a_removed_node_is_no_longer_in_the_list() {
  let mut list = DoubleLinkedList::new();
  let a = list.append("a");
  let b = list.append("b");
  assert_eq!(list.remove(a), "a");
  assert_eq!(list.get(a), None);
  assert_eq!(list.get(b), Some(&"b"));
  assert_eq!(list.get_previous(b), None);
  assert_eq!(values(&list), ["b"]);
}

#[test]
fn a_removed_id_is_not_reused_by_a_later_insert() {
  let mut list = DoubleLinkedList::new();
  let a = list.append("a");
  list.remove(a);
  let again = list.append("a");
  assert_ne!(again, a);
  assert_eq!(list.get(a), None);
}

#[test]
#[should_panic(expected = "the node is in this list")]
fn moving_a_removed_node_panics() {
  let mut list = DoubleLinkedList::new();
  let a = list.append("a");
  list.append("b");
  list.remove(a);
  list.move_to_front(a);
}

#[test]
fn previous_and_next_follow_the_links() {
  let mut list = DoubleLinkedList::new();
  let a = list.append("a");
  let b = list.append("b");
  let c = list.append("c");
  assert_eq!((list.get_previous(a), list.get_next(a)), (None, Some(b)));
  assert_eq!((list.get_previous(b), list.get_next(b)), (Some(a), Some(c)));
  assert_eq!((list.get_previous(c), list.get_next(c)), (Some(b), None));
  list.move_to_front(c);
  assert_eq!((list.get_previous(c), list.get_next(c)), (None, Some(a)));
  assert_eq!((list.get_previous(b), list.get_next(b)), (Some(a), None));
}

#[test]
fn the_new_value_is_made_only_when_a_node_is_inserted() {
  let mut list = DoubleLinkedList::new();
  let a = list.append("a");
  let b = list.append("b");
  let unused = || -> &'static str { panic!("an existing node was returned, so no new value is made") };
  assert_eq!(list.get_first_or_else_prepend(|value| *value == "a", unused), a);
  assert_eq!(list.get_last_or_else_append(|value| *value == "b", unused), b);
  assert_eq!(list.get_next_or_else_insert(a, |value| *value == "b", unused), b);
  assert_eq!(list.get_previous_or_else_insert(b, |value| *value == "a", unused), a);
  assert_eq!(values(&list), ["a", "b"]);
}

#[test]
fn find_stops_at_the_end_of_the_list_and_inserts_there() {
  let mut list = DoubleLinkedList::new();
  let a = list.append("a1");
  list.append("a2");
  let inserted = list.find_next_or_else_insert(a, |value| value.starts_with('a'), |_| false, || "b1");
  assert_eq!(list.get_last(), Some(inserted));
  let first = list.get_first().expect("not empty");
  let front = list.find_previous_or_else_insert(first, |_| true, |_| false, || "a0");
  assert_eq!(list.get_first(), Some(front));
  assert_eq!(values(&list), ["a0", "a1", "a2", "b1"]);
}

#[test]
fn index_mut_changes_the_value_in_place() {
  let mut list = DoubleLinkedList::new();
  let a = list.append("a");
  list[a] = "z";
  *list.get_mut(a).expect("in the list") = "y";
  assert_eq!(values(&list), ["y"]);
}

#[test]
fn an_id_from_another_list_does_not_name_a_node_here() {
  let mut mine = DoubleLinkedList::new();
  let mut other = DoubleLinkedList::new();
  let foreign = other.append("other");
  let a = mine.append("a");
  assert_ne!(foreign, a);
  assert_eq!(mine.get(foreign), None);
  assert_eq!(mine.get_mut(foreign), None);
  assert_eq!(values(&mine), ["a"]);
}

#[test]
#[should_panic(expected = "the node is in this list")]
fn removing_an_id_from_another_list_panics() {
  let mut mine = DoubleLinkedList::new();
  let mut other = DoubleLinkedList::new();
  let foreign = other.append("other");
  mine.append("a");
  mine.remove(foreign);
}
