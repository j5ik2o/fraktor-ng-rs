//! A FIFO buffer for messages that cannot be delivered yet.

use alloc::collections::VecDeque;

/// A first-in-first-out buffer of `(message, sender)` pairs.
///
/// Mirrors `org.apache.pekko.util.MessageBuffer` (marked `@InternalApi` in
/// Pekko), the buffer Pekko's stash and dead-letter code hold undeliverable
/// mail in. The original buffers `(Any, ActorRef)` pairs behind a hand-rolled
/// linked list; the port is generic over the message and the sender and sits
/// on a `VecDeque`, so the original's manual link keeping has no counterpart
/// here. [`head`](Self::head) returns an `Option` where the original returns
/// `(null, null)` for an empty buffer.
#[derive(Debug, Clone)]
pub struct MessageBuffer<M, S> {
  entries: VecDeque<(M, S)>,
}

impl<M, S> Default for MessageBuffer<M, S> {
  fn default() -> Self {
    Self::empty()
  }
}

impl<M, S> MessageBuffer<M, S> {
  /// Creates an empty buffer.
  #[must_use]
  pub fn empty() -> Self {
    Self { entries: VecDeque::new() }
  }

  /// Returns whether the buffer holds no entry.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }

  /// Returns the number of entries in the buffer.
  #[must_use]
  pub fn size(&self) -> usize {
    self.entries.len()
  }

  /// Appends `(message, sender)` at the tail and returns the buffer, so
  /// calls can be chained as in the original.
  pub fn append(&mut self, message: M, sender: S) -> &mut Self {
    self.entries.push_back((message, sender));
    self
  }

  /// Drops the head entry. Dropping the head of an empty buffer does
  /// nothing, as in the original.
  pub fn drop_head(&mut self) {
    self.entries.pop_front();
  }

  /// Returns the head entry without dropping it, or `None` when the buffer
  /// is empty.
  #[must_use]
  pub fn head(&self) -> Option<(&M, &S)> {
    self.entries.front().map(|(message, sender)| (message, sender))
  }

  /// Calls `f` once for every entry, in the order they were appended.
  pub fn for_each<F>(&self, mut f: F)
  where
    F: FnMut(&M, &S),
  {
    for (message, sender) in &self.entries {
      f(message, sender);
    }
  }

  /// Drops every entry for which `f` returns `true`, keeping the order of
  /// the rest.
  pub fn filter_not<F>(&mut self, mut f: F)
  where
    F: FnMut(&M, &S) -> bool,
  {
    self.entries.retain(|(message, sender)| !f(message, sender));
  }
}
