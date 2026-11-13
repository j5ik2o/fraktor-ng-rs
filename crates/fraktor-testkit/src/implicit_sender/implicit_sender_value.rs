//! The sender a test uses when it sends a message without naming one.

use fraktor_actor::kernel::actor_ref::ActorRef;

use crate::fraktorspec::FraktorSpec;

/// Provides the sender that a test uses implicitly.
///
/// Mirrors `org.apache.pekko.testkit.ImplicitSender`, whose
/// `implicit def self: ActorRef` is `testActor`. The Rust port cannot mix a
/// trait into a test type implicitly, so the handle is produced by a function
/// that takes the spec of the test.
pub struct ImplicitSender;

impl ImplicitSender {
  /// Returns the test actor of the given spec, which is the implicit sender.
  pub fn self_actor_ref(spec: &FraktorSpec) -> ActorRef {
    spec.test_actor()
  }
}
