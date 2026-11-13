//! The outcome of running a unit of work that may fail.

use alloc::string::String;

/// What running a piece of actor work produced.
///
/// Pekko catches the exception a failing actor throws and keeps it as the cause
/// of the failure it reports. The kernel cannot catch a panic, so the port asks
/// a [`FailureCatcher`](crate::kernel::supervision::FailureCatcher) to run the
/// work and to say whether it completed or failed. The failure carries a
/// message, which is the payload of the panic that the failing actor raised,
/// so an actor can report the cause of its own failure the way a test expects.
pub enum RunOutcome {
  /// The work completed without failing.
  Completed,
  /// The work failed, with the message that describes the failure.
  Failed {
    /// The message of the failure, or an empty string when it has none.
    message: String,
  },
}
