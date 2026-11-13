//! The policy that a test actor applies to each message before queuing it.

use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;

use crate::test_kit::AutoPilotControl;

/// Reacts to a message before the test actor queues it.
///
/// Mirrors `org.apache.pekko.testkit.TestKit.TestActor.AutoPilot`, whose
/// `run(sender, msg)` returns either `KeepRunning` or `NoAutoPilot`. The port
/// returns a [`AutoPilotControl`] for the same two outcomes. Pekko also lets a
/// pilot return another pilot in place of `KeepRunning`; the port keeps the two
/// named outcomes that the ported tests use.
pub trait AutoPilot: Send {
  /// Runs for one message and returns whether the pilot stays installed.
  fn run(&mut self, sender: &ActorRef, message: Message) -> AutoPilotControl;
}
