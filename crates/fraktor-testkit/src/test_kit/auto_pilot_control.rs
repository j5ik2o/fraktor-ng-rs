//! What an auto pilot asks the test actor to do next.

/// The outcome of one auto pilot run.
///
/// Mirrors the two values of `org.apache.pekko.testkit.TestKit.TestActor` that
/// a pilot may return: `KeepRunning`, which keeps the pilot installed, and
/// `NoAutoPilot`, which removes it so later messages are only queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoPilotControl {
  /// Keeps the current pilot for the next message.
  KeepRunning,
  /// Removes the current pilot.
  NoAutoPilot,
}
