// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2021-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A mutable doubly linked list whose nodes are addressed by id.

use alloc::collections::BTreeMap;
use core::ops::{Index, IndexMut};

use portable_atomic::{AtomicU64, Ordering};

use super::NodeId;

#[cfg(test)]
#[path = "double_linked_list_value_test.rs"]
mod tests;

// Numbers each list, so that the ids of one list never name a node of another.
static NEXT_LIST: AtomicU64 = AtomicU64::new(0);

/// A mutable, non-thread-safe doubly linked list.
///
/// Mirrors `org.apache.pekko.util.DoubleLinkedList`, the internal list that
/// Pekko's recency and frequency lists build on. Pekko's list is intrusive: the
/// caller's node type carries its own previous and next links, and the list is
/// built from the four functions that read and write them. Rust has no shape
/// that matches that one to one, so the port owns its values and keeps the
/// links itself. Inserting a value returns the [`NodeId`] of its node, and the
/// operations that take or return a node in Pekko take or return that id.
/// [`get`](Self::get), [`get_mut`](Self::get_mut) and indexing reach a node's
/// value, and [`get_previous`](Self::get_previous) and
/// [`get_next`](Self::get_next) stand for the link getters Pekko is built with.
///
/// The operations that may insert a new node take a closure that makes its
/// value, which runs only when the node is inserted, as Pekko's by-name
/// `newNode: => Node` is evaluated only then.
///
/// [`remove`](Self::remove) takes the node out of the list and returns its
/// value. Pekko keeps the unlinked node object, which the caller can insert
/// again; here the caller inserts the returned value again and gets a new id.
///
/// An id names a node of the list that returned it. The operations that take an
/// id panic when it does not name a node in this list (it was removed, or it
/// came from another list), where Pekko's list would follow the stale links of
/// the node it was given. Nodes are kept in an ordered map by id, so reaching a
/// node takes logarithmic time instead of Pekko's constant time.
///
/// A list numbers its nodes from zero and never reuses a number. An insertion
/// that would need a 2^64th number panics instead of wrapping around and
/// overwriting a live node.
pub struct DoubleLinkedList<A> {
  nodes: BTreeMap<NodeId, Node<A>>,
  first: Option<NodeId>,
  last: Option<NodeId>,
  list: u64,
  next_id: u64,
}

struct Node<A> {
  value: A,
  previous: Option<NodeId>,
  next: Option<NodeId>,
}

impl<A> DoubleLinkedList<A> {
  /// Creates an empty list.
  ///
  /// # Panics
  ///
  /// Panics when 2^64 lists have already been created, so that two lists never
  /// share a number.
  #[must_use]
  pub fn new() -> Self {
    let list = NEXT_LIST
      .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |list| list.checked_add(1))
      .expect("fewer than 2^64 lists are created");
    Self { nodes: BTreeMap::new(), first: None, last: None, list, next_id: 0 }
  }

  /// Returns whether the list has no nodes.
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.first.is_none()
  }

  /// Returns the first node, if any.
  #[must_use]
  pub const fn get_first(&self) -> Option<NodeId> {
    self.first
  }

  /// Returns the last node, if any.
  #[must_use]
  pub const fn get_last(&self) -> Option<NodeId> {
    self.last
  }

  /// Returns the value of `node`, or `None` when `node` is not in this list.
  #[must_use]
  pub fn get(&self, node: NodeId) -> Option<&A> {
    self.nodes.get(&node).map(|entry| &entry.value)
  }

  /// Returns the value of `node` mutably, or `None` when `node` is not in this
  /// list.
  pub fn get_mut(&mut self, node: NodeId) -> Option<&mut A> {
    self.nodes.get_mut(&node).map(|entry| &mut entry.value)
  }

  /// Returns the node before `node`, if any.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  #[must_use]
  pub fn get_previous(&self, node: NodeId) -> Option<NodeId> {
    self.node(node).previous
  }

  /// Returns the node after `node`, if any.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  #[must_use]
  pub fn get_next(&self, node: NodeId) -> Option<NodeId> {
    self.node(node).next
  }

  /// Inserts `value` at the front and returns its node.
  pub fn prepend(&mut self, value: A) -> NodeId {
    let node = self.allocate(value);
    self.insert_before(node, self.first)
  }

  /// Inserts `value` at the back and returns its node.
  pub fn append(&mut self, value: A) -> NodeId {
    let node = self.allocate(value);
    self.insert_after(self.last, node)
  }

  /// Takes `node` out of the list and returns its value.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  pub fn remove(&mut self, node: NodeId) -> A {
    self.unlink(node);
    self.nodes.remove(&node).expect("the node is in this list").value
  }

  /// Moves `node` to the front and returns it. A node already at the front
  /// stays where it is.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  pub fn move_to_front(&mut self, node: NodeId) -> NodeId {
    if self.first == Some(node) {
      node
    } else {
      self.unlink(node);
      self.insert_before(node, self.first)
    }
  }

  /// Moves `node` to the back and returns it. A node already at the back stays
  /// where it is.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  pub fn move_to_back(&mut self, node: NodeId) -> NodeId {
    if self.last == Some(node) {
      node
    } else {
      self.unlink(node);
      self.insert_after(self.last, node)
    }
  }

  /// Returns the first node when its value passes `check`, and otherwise
  /// inserts the value `new_value` makes at the front and returns its node.
  pub fn get_first_or_else_prepend(&mut self, check: impl FnOnce(&A) -> bool, new_value: impl FnOnce() -> A) -> NodeId {
    match self.first {
      | Some(first) if check(&self.node(first).value) => first,
      | _ => self.prepend(new_value()),
    }
  }

  /// Returns the last node when its value passes `check`, and otherwise inserts
  /// the value `new_value` makes at the back and returns its node.
  pub fn get_last_or_else_append(&mut self, check: impl FnOnce(&A) -> bool, new_value: impl FnOnce() -> A) -> NodeId {
    match self.last {
      | Some(last) if check(&self.node(last).value) => last,
      | _ => self.append(new_value()),
    }
  }

  /// Returns the node after `node` when its value passes `check`, and otherwise
  /// inserts the value `new_value` makes right after `node` and returns its
  /// node.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  pub fn get_next_or_else_insert(
    &mut self,
    node: NodeId,
    check: impl FnOnce(&A) -> bool,
    new_value: impl FnOnce() -> A,
  ) -> NodeId {
    match self.node(node).next {
      | Some(next) if check(&self.node(next).value) => next,
      | _ => {
        let inserted = self.allocate(new_value());
        self.insert_after(Some(node), inserted)
      },
    }
  }

  /// Returns the node before `node` when its value passes `check`, and
  /// otherwise inserts the value `new_value` makes right before `node` and
  /// returns its node.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  pub fn get_previous_or_else_insert(
    &mut self,
    node: NodeId,
    check: impl FnOnce(&A) -> bool,
    new_value: impl FnOnce() -> A,
  ) -> NodeId {
    match self.node(node).previous {
      | Some(previous) if check(&self.node(previous).value) => previous,
      | _ => {
        let inserted = self.allocate(new_value());
        self.insert_before(inserted, Some(node))
      },
    }
  }

  /// Walks forward from `node` while the next value passes `is_before`, then
  /// does [`get_next_or_else_insert`](Self::get_next_or_else_insert) from the
  /// node it stopped at.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  pub fn find_next_or_else_insert(
    &mut self,
    node: NodeId,
    is_before: impl FnMut(&A) -> bool,
    check: impl FnOnce(&A) -> bool,
    new_value: impl FnOnce() -> A,
  ) -> NodeId {
    let start = self.shift_while(node, |entry| entry.next, is_before);
    self.get_next_or_else_insert(start, check, new_value)
  }

  /// Walks backward from `node` while the previous value passes `is_after`,
  /// then does [`get_previous_or_else_insert`](Self::get_previous_or_else_insert)
  /// from the node it stopped at.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  pub fn find_previous_or_else_insert(
    &mut self,
    node: NodeId,
    is_after: impl FnMut(&A) -> bool,
    check: impl FnOnce(&A) -> bool,
    new_value: impl FnOnce() -> A,
  ) -> NodeId {
    let start = self.shift_while(node, |entry| entry.previous, is_after);
    self.get_previous_or_else_insert(start, check, new_value)
  }

  /// Iterates the nodes from the first to the last.
  pub fn forward_iter(&self) -> impl Iterator<Item = NodeId> + '_ {
    core::iter::successors(self.first, |node| self.node(*node).next)
  }

  /// Iterates the nodes from the last to the first.
  pub fn backward_iter(&self) -> impl Iterator<Item = NodeId> + '_ {
    core::iter::successors(self.last, |node| self.node(*node).previous)
  }

  fn node(&self, node: NodeId) -> &Node<A> {
    self.nodes.get(&node).expect("the node is in this list")
  }

  fn node_mut(&mut self, node: NodeId) -> &mut Node<A> {
    self.nodes.get_mut(&node).expect("the node is in this list")
  }

  fn allocate(&mut self, value: A) -> NodeId {
    let node = NodeId::new(self.list, self.next_id);
    self.next_id = self.next_id.checked_add(1).expect("a list allocates fewer than 2^64 nodes");
    self.nodes.insert(node, Node { value, previous: None, next: None });
    node
  }

  fn insert_before(&mut self, node: NodeId, next: Option<NodeId>) -> NodeId {
    let previous = next.and_then(|next| self.node(next).previous);
    self.link(previous, node, next);
    node
  }

  fn insert_after(&mut self, previous: Option<NodeId>, node: NodeId) -> NodeId {
    let next = previous.and_then(|previous| self.node(previous).next);
    self.link(previous, node, next);
    node
  }

  fn link(&mut self, previous: Option<NodeId>, node: NodeId, next: Option<NodeId>) {
    let entry = self.node_mut(node);
    entry.previous = previous;
    entry.next = next;
    match previous {
      | Some(previous) => self.node_mut(previous).next = Some(node),
      | None => self.first = Some(node),
    }
    match next {
      | Some(next) => self.node_mut(next).previous = Some(node),
      | None => self.last = Some(node),
    }
  }

  fn unlink(&mut self, node: NodeId) {
    let entry = self.node_mut(node);
    let previous = entry.previous.take();
    let next = entry.next.take();
    match previous {
      | Some(previous) => self.node_mut(previous).next = next,
      | None => self.first = next,
    }
    match next {
      | Some(next) => self.node_mut(next).previous = previous,
      | None => self.last = previous,
    }
  }

  fn shift_while(
    &self,
    start: NodeId,
    shift: impl Fn(&Node<A>) -> Option<NodeId>,
    mut check: impl FnMut(&A) -> bool,
  ) -> NodeId {
    let mut current = start;
    while let Some(peek) = shift(self.node(current)) {
      if !check(&self.node(peek).value) {
        break;
      }
      current = peek;
    }
    current
  }
}

impl<A> Default for DoubleLinkedList<A> {
  fn default() -> Self {
    Self::new()
  }
}

impl<A> Index<NodeId> for DoubleLinkedList<A> {
  type Output = A;

  /// Returns the value of `node`.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  fn index(&self, node: NodeId) -> &A {
    &self.node(node).value
  }
}

impl<A> IndexMut<NodeId> for DoubleLinkedList<A> {
  /// Returns the value of `node` mutably.
  ///
  /// # Panics
  ///
  /// Panics when `node` is not in this list.
  fn index_mut(&mut self, node: NodeId) -> &mut A {
    &mut self.node_mut(node).value
  }
}
