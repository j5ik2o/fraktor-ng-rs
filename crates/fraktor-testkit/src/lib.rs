//! Test support for ported Pekko actor tests.
//!
//! This crate mirrors the parts of `org.apache.pekko.testkit` that the ported
//! tests need. G0b seeds it with the test kit settings, the `DefaultTimeout`
//! trait, the `ImplicitSender` sender, and the `FraktorSpec` base that owns an
//! actor system and its test actor.

pub mod default_timeout;
pub mod event_filter;
pub mod fraktorspec;
pub mod implicit_sender;
pub mod test_actors;
pub mod test_kit;
pub mod test_kit_extension;
