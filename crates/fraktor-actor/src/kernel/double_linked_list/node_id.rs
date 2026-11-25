//! The id of a node in a [`DoubleLinkedList`](super::DoubleLinkedList).

/// Names one node of a [`DoubleLinkedList`](super::DoubleLinkedList).
///
/// Pekko's `DoubleLinkedList[Node]` links the caller's own node objects, so the
/// node itself is what its operations take and return. The port owns its
/// values, so inserting a value returns a `NodeId` for it, and the operations
/// that take or return a node in Pekko take or return its id. Two ids are equal
/// exactly when they name the same node, which is how a caller checks that an
/// operation returned an existing node. A list never reuses the id of a removed
/// node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u64);

impl NodeId {
  pub(crate) const fn new(id: u64) -> Self {
    Self(id)
  }
}
