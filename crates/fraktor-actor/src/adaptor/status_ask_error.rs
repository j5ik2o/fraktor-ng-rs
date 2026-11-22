//! Failures observed when waiting for a classic status reply.

use alloc::sync::Arc;
use core::error::Error;
use core::fmt;

use super::AskError;

/// An ask timeout, an error sent by the recipient, or an unexpected reply.
#[derive(Debug)]
pub enum StatusAskError {
  /// The ask's deadline or the caller's wait limit elapsed.
  Ask(AskError),
  /// The recipient sent a failed status reply with this original error.
  Reply(Arc<dyn Error + Send + Sync>),
  /// The recipient sent a message without a [status reply](super::StatusReply) envelope.
  UnexpectedReply,
}

impl fmt::Display for StatusAskError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::Ask(AskError::AskTimedOut) => formatter.write_str("ask deadline expired"),
      | Self::Ask(AskError::WaitTimedOut) => formatter.write_str("ask wait limit expired"),
      | Self::Reply(error) => fmt::Display::fmt(error, formatter),
      | Self::UnexpectedReply => formatter.write_str("expected a status reply"),
    }
  }
}

impl Error for StatusAskError {
  fn source(&self) -> Option<&(dyn Error + 'static)> {
    match self {
      | Self::Reply(error) => Some(error.as_ref()),
      | _ => None,
    }
  }
}
