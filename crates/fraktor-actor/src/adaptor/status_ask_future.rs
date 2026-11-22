//! Waiting for the value of a classic status reply.

use core::any::Any;
use core::time::Duration;

use super::AskFuture;
use super::StatusAskError;
use super::StatusReply;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;
use crate::kernel::timeout::Timeout;

/// A deadline-bound classic ask that unwraps the recipient's status reply.
#[derive(Debug)]
pub struct StatusAskFuture {
  ask: AskFuture,
}

impl StatusAskFuture {
  /// Waits for the reply up to the given limit and returns its success value or failure.
  ///
  /// The underlying ask tracks its own deadline separately from this wait.
  pub fn try_await_result(&self, max: Duration) -> Result<Message, StatusAskError> {
    let reply = self.ask.try_await_result(max).map_err(StatusAskError::Ask)?;
    let status = reply.downcast_ref::<StatusReply>().ok_or(StatusAskError::UnexpectedReply)?;
    status.result().map_err(StatusAskError::Reply)
  }
}

impl ActorRef {
  /// Sends a question using the classic ask path and expects a status reply.
  pub fn ask_with_status(&self, message: impl Any + Send + Sync, timeout: Timeout) -> StatusAskFuture {
    StatusAskFuture { ask: self.ask_with_timeout(message, timeout) }
  }
}
