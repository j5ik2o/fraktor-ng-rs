//! Success and failure envelopes for classic actor replies.

use alloc::sync::Arc;
use core::any::Any;
use core::error::Error;
use core::future::Future;

use crate::adaptor::ErrorMessage;
use crate::adaptor::StatusReplyAck;
use crate::kernel::message::Message;

/// A classic actor reply carrying either a value or a recoverable error.
///
/// The value uses classic actor message type erasure. An error keeps its
/// concrete type so a recipient can downcast it.
#[derive(Debug)]
pub struct StatusReply {
  result: Result<Message, Arc<dyn Error + Send + Sync>>,
}

impl StatusReply {
  /// Creates a successful reply carrying the given value.
  pub fn success(value: impl Any + Send + Sync) -> Self {
    Self { result: Ok(Message::new(value)) }
  }

  /// Creates a successful reply carrying the [StatusReplyAck] acknowledgement.
  pub fn ack() -> Self {
    Self::success(StatusReplyAck)
  }

  /// Creates a failed reply whose error is an [ErrorMessage].
  pub fn error_message(message: impl Into<String>) -> Self {
    Self::error(ErrorMessage(message.into()))
  }

  /// Creates a failed reply preserving the original error and its concrete type.
  pub fn error(error: impl Error + Send + Sync + 'static) -> Self {
    Self { result: Err(Arc::new(error)) }
  }

  /// Returns whether this reply carries a successful value.
  pub fn is_success(&self) -> bool {
    self.result.is_ok()
  }

  /// Returns whether this reply carries an error.
  pub fn is_error(&self) -> bool {
    self.result.is_err()
  }

  /// Returns the successful value or the error carried by the reply.
  pub fn get_value(&self) -> Result<&Message, &(dyn Error + Send + Sync + 'static)> {
    self.result.as_ref().map_err(Arc::as_ref)
  }

  /// Returns the carried error, if this is a failed reply.
  pub fn get_error(&self) -> Option<&(dyn Error + Send + Sync + 'static)> {
    self.get_value().err()
  }

  /// Returns the outcome of a present reply. An absent reply matches neither outcome.
  pub fn match_reply(reply: Option<&Self>) -> Option<Result<&Message, &(dyn Error + Send + Sync + 'static)>> {
    reply.map(Self::get_value)
  }

  pub(super) fn result(&self) -> Result<Message, Arc<dyn Error + Send + Sync>> {
    self.result.clone()
  }

  /// Unwraps a future that delivers a [StatusReply].
  ///
  /// Returns the successful value or propagates the original error from either
  /// the future or the reply.
  pub async fn flatten_status_future(
    future: impl Future<Output = Result<Self, Arc<dyn Error + Send + Sync>>>,
  ) -> Result<Message, Arc<dyn Error + Send + Sync>> {
    future.await?.result
  }
}
