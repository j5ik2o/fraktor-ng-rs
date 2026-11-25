//! The limits of a `receive_while` call.

use core::time::Duration;

/// The limits that end a `receive_while` call, besides a message that does not
/// match.
///
/// Mirrors the parameters of `TestKitBase.receiveWhile(max, idle, messages)`
/// and their defaults: without `max` the call uses the time left in the
/// enclosing `within`, or the dilated single-expect default outside one;
/// without `idle` there is no limit between two messages; without `messages`
/// there is no limit on the count. A `max` that is given is dilated by the time
/// factor, as Pekko dilates it; `idle` is not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReceiveWhile {
  max: Option<Duration>,
  idle: Option<Duration>,
  messages: Option<usize>,
}

impl ReceiveWhile {
  /// Returns the limits with every default: no `max`, no `idle`, and no limit
  /// on the count.
  pub const fn new() -> Self {
    Self { max: None, idle: None, messages: None }
  }

  /// Ends the call once `max`, dilated, has passed since it started.
  #[must_use]
  pub const fn max(mut self, max: Duration) -> Self {
    self.max = Some(max);
    self
  }

  /// Ends the call when no message arrives for `idle`.
  #[must_use]
  pub const fn idle(mut self, idle: Duration) -> Self {
    self.idle = Some(idle);
    self
  }

  /// Ends the call once `messages` messages have matched.
  #[must_use]
  pub const fn messages(mut self, messages: usize) -> Self {
    self.messages = Some(messages);
    self
  }

  pub(crate) const fn max_limit(self) -> Option<Duration> {
    self.max
  }

  pub(crate) const fn idle_limit(self) -> Option<Duration> {
    self.idle
  }

  pub(crate) fn message_limit(self) -> usize {
    self.messages.unwrap_or(usize::MAX)
  }
}
