//! A broadcaster's subscriber set and the messages that manage and use it.
//!
//! Mirrors `org.apache.pekko.routing.Listeners`: an actor owns the set of
//! subscribers, updates it through the management messages, applies an
//! operation to each current subscriber, and delivers a gossip message to them.
//! The port keeps only that actor-local set; it has no system-wide table and
//! does not watch the subscribers.

mod subscribe;
mod subscriber_registry;
mod unsubscribe;
mod with_subscribers;

pub use subscribe::Subscribe;
pub use subscriber_registry::Subscribers;
pub use unsubscribe::Unsubscribe;
pub use with_subscribers::WithSubscribers;
