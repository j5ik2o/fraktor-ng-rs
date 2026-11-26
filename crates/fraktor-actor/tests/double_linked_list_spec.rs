// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2016-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/DoubleLinkedListSpec.scala
//
// The original's twelve cases under "DoubleLinkedList must" are kept as one
// test each, named after the case, with the original name as a comment. Every
// operation runs in the original order, and each `check` stays where the
// original puts it.
//
// The original list is intrusive: its `Node` carries its own `less` and `more`
// links, and the list is built with the four functions that read and write
// them. Rust has no shape that matches that one to one, so the port's
// `DoubleLinkedList<A>` owns its values and keeps the links itself, the way
// `RecencyList` and `FrequencyList` keep theirs by id. Inserting a value
// returns its `NodeId`, and the operations that take a node in the original
// take that id. The original's `Node(value)` is therefore just its value here,
// and a check that the original returns "the same node" (`shouldBe b`) checks
// that the port returns the same `NodeId`. Where the original passes a node
// that is not in the list yet as the new node, the port passes a closure that
// makes the value (the original's by-name `newNode: => Node`), and the test
// checks the value at the returned id and the list's order.
//
// The original's `check` reads `getFirst` and `getLast` as `OptionVal[Node]`
// and maps them to the value. The port's `get_first` and `get_last` return the
// `NodeId`, which the test maps to the value through the list.
//
// The test calls the production `DoubleLinkedList`. Until it exists the file
// does not compile: that compile error is the intended red for this node.

use fraktor_actor::kernel::double_linked_list::DoubleLinkedList;

type List = DoubleLinkedList<&'static str>;

fn create() -> List {
  DoubleLinkedList::new()
}

fn check(list: &List, expected: &[&'static str]) {
  assert_eq!(list.is_empty(), expected.is_empty());
  assert_eq!(list.get_first().map(|id| list[id]), expected.first().copied());
  assert_eq!(list.get_last().map(|id| list[id]), expected.last().copied());
  assert_eq!(list.forward_iter().map(|id| list[id]).collect::<Vec<_>>(), expected);
  assert_eq!(
    list.backward_iter().map(|id| list[id]).collect::<Vec<_>>(),
    expected.iter().rev().copied().collect::<Vec<_>>()
  );
}

// "start empty"
#[test]
fn start_empty() {
  let list = create();
  check(&list, &[]);
}

// "prepend elements to front"
#[test]
fn prepend_elements_to_front() {
  let mut list = create();
  list.prepend("c");
  list.prepend("b");
  list.prepend("a");
  check(&list, &["a", "b", "c"]);
}

// "append elements to back"
#[test]
fn append_elements_to_back() {
  let mut list = create();
  list.append("a");
  list.append("b");
  list.append("c");
  check(&list, &["a", "b", "c"]);
}

// "remove elements"
#[test]
fn remove_elements() {
  let mut list = create();
  let a = list.prepend("a");
  let b = list.append("b");
  let c = list.append("c");
  check(&list, &["a", "b", "c"]);
  list.remove(b);
  check(&list, &["a", "c"]);
  list.remove(c);
  check(&list, &["a"]);
  list.remove(a);
  check(&list, &[]);
}

// "move elements to front"
#[test]
fn move_elements_to_front() {
  let mut list = create();
  let a = list.prepend("a");
  let b = list.prepend("b");
  let c = list.prepend("c");
  check(&list, &["c", "b", "a"]);
  list.move_to_front(c);
  check(&list, &["c", "b", "a"]);
  list.move_to_front(b);
  check(&list, &["b", "c", "a"]);
  list.move_to_front(a);
  check(&list, &["a", "b", "c"]);
}

// "move elements to back"
#[test]
fn move_elements_to_back() {
  let mut list = create();
  let c = list.append("c");
  let b = list.append("b");
  let a = list.append("a");
  check(&list, &["c", "b", "a"]);
  list.move_to_back(a);
  check(&list, &["c", "b", "a"]);
  list.move_to_back(b);
  check(&list, &["c", "a", "b"]);
  list.move_to_back(c);
  check(&list, &["a", "b", "c"]);
}

// "get first element or else prepend new element"
#[test]
fn get_first_element_or_else_prepend_new_element() {
  let mut list = create();
  list.prepend("c");
  let b = list.prepend("b");
  check(&list, &["b", "c"]);
  assert_eq!(list.get_first_or_else_prepend(|value| *value == "b", || "a"), b);
  check(&list, &["b", "c"]);
  let a = list.get_first_or_else_prepend(|value| *value == "a", || "a");
  assert_eq!(list[a], "a");
  check(&list, &["a", "b", "c"]);
}

// "get last element or else append new element"
#[test]
fn get_last_element_or_else_append_new_element() {
  let mut list = create();
  list.append("a");
  let b = list.append("b");
  check(&list, &["a", "b"]);
  assert_eq!(list.get_last_or_else_append(|value| *value == "b", || "c"), b);
  check(&list, &["a", "b"]);
  let c = list.get_last_or_else_append(|value| *value == "c", || "c");
  assert_eq!(list[c], "c");
  check(&list, &["a", "b", "c"]);
}

// "get next element or else insert new element"
#[test]
fn get_next_element_or_else_insert_new_element() {
  let mut list = create();
  let c = list.prepend("c");
  let a = list.prepend("a");
  check(&list, &["a", "c"]);
  assert_eq!(list.get_next_or_else_insert(a, |value| *value == "c", || "b"), c);
  check(&list, &["a", "c"]);
  let b = list.get_next_or_else_insert(a, |value| *value == "b", || "b");
  assert_eq!(list[b], "b");
  check(&list, &["a", "b", "c"]);
}

// "get previous element or else insert new element"
#[test]
fn get_previous_element_or_else_insert_new_element() {
  let mut list = create();
  let a = list.append("a");
  let c = list.append("c");
  check(&list, &["a", "c"]);
  assert_eq!(list.get_previous_or_else_insert(c, |value| *value == "a", || "b"), a);
  check(&list, &["a", "c"]);
  let b = list.get_previous_or_else_insert(c, |value| *value == "b", || "b");
  assert_eq!(list[b], "b");
  check(&list, &["a", "b", "c"]);
}

// "find next element or else insert new element"
#[test]
fn find_next_element_or_else_insert_new_element() {
  let mut list = create();
  list.prepend("c2");
  let c1 = list.prepend("c1");
  list.prepend("a2");
  let a1 = list.prepend("a1");
  check(&list, &["a1", "a2", "c1", "c2"]);
  assert_eq!(
    list.find_next_or_else_insert(a1, |value| value.starts_with('a'), |value| value.starts_with('c'), || "b1"),
    c1
  );
  check(&list, &["a1", "a2", "c1", "c2"]);
  let b1 = list.find_next_or_else_insert(a1, |value| value.starts_with('a'), |value| value.starts_with('b'), || "b1");
  assert_eq!(list[b1], "b1");
  check(&list, &["a1", "a2", "b1", "c1", "c2"]);
  let d1 = list.find_next_or_else_insert(c1, |value| value.starts_with('c'), |value| value.starts_with('d'), || "d1");
  assert_eq!(list[d1], "d1");
  check(&list, &["a1", "a2", "b1", "c1", "c2", "d1"]);
}

// "find previous element or else insert new element"
#[test]
fn find_previous_element_or_else_insert_new_element() {
  let mut list = create();
  list.append("b1");
  let b2 = list.append("b2");
  list.append("d1");
  let d2 = list.append("d2");
  check(&list, &["b1", "b2", "d1", "d2"]);
  assert_eq!(
    list.find_previous_or_else_insert(d2, |value| value.starts_with('d'), |value| value.starts_with('b'), || "c1"),
    b2
  );
  check(&list, &["b1", "b2", "d1", "d2"]);
  let c1 =
    list.find_previous_or_else_insert(d2, |value| value.starts_with('d'), |value| value.starts_with('c'), || "c1");
  assert_eq!(list[c1], "c1");
  check(&list, &["b1", "b2", "c1", "d1", "d2"]);
  let a1 =
    list.find_previous_or_else_insert(b2, |value| value.starts_with('b'), |value| value.starts_with('a'), || "a1");
  assert_eq!(list[a1], "a1");
  check(&list, &["a1", "b1", "b2", "c1", "d1", "d2"]);
}
