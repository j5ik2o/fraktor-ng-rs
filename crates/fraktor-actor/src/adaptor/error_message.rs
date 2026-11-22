//! Errors created from the text of a failed classic status reply.

use core::error::Error;
use core::fmt;

/// The concrete error carried by a text-based [status reply](super::StatusReply).
#[derive(Debug)]
pub struct ErrorMessage(pub(super) String);

impl fmt::Display for ErrorMessage {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl Error for ErrorMessage {}
