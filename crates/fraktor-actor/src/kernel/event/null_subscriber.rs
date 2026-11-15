//! The rejection of a null event stream subscriber.

use core::fmt;

/// The rejection of a null event stream subscriber.
///
/// Mirrors the `IllegalArgumentException("subscriber is null")` that
/// `org.apache.pekko.event.EventStream.subscribe` throws when it is given
/// `null`. The port makes the null case explicit in the type, so a caller that
/// passes `None` learns about the refusal instead of subscribing nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NullSubscriber;

impl fmt::Display for NullSubscriber {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("subscriber is null")
  }
}
