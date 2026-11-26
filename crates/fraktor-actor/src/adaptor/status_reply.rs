// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2020-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! Success and failure envelopes for classic actor replies.

use alloc::sync::Arc;
use core::any::Any;
use core::error::Error;
use core::fmt;
use core::future::Future;

use crate::adaptor::ErrorMessage;
use crate::adaptor::StatusReplyAck;
use crate::kernel::message::Message;

#[cfg(test)]
#[path = "status_reply_test.rs"]
mod tests;

/// A classic actor reply carrying either a value or a recoverable error.
///
/// The value uses classic actor message type erasure. An error keeps its
/// concrete type so a recipient can downcast it.
///
/// Its `Debug` is the reply's rendering: `Success(<value>)`, `Error(<text>)` for
/// a text error, and `Error(<type>: <error's Debug>)` for any other error.
/// Pekko's `toString` renders `Error(<exception>)` and leaves the type to the
/// exception's own `toString`, which on the JVM normally starts with the class
/// name. The port names the type itself, as [`core::any::type_name`] of the
/// error given to [`error`](Self::error), and renders the error only through
/// its own `Debug`, so an error that hides its value from `Debug` stays hidden.
/// The type name is the compiler's rendering and is not a stable string.
pub struct StatusReply {
  result: Result<Message, Arc<dyn Error + Send + Sync>>,
  // The type name of the error, recorded where its concrete type is known.
  error_type: Option<&'static str>,
}

impl StatusReply {
  /// Creates a successful reply carrying the given value.
  pub fn success(value: impl Any + Send + Sync) -> Self {
    Self { result: Ok(Message::new(value)), error_type: None }
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
  pub fn error<E: Error + Send + Sync + 'static>(error: E) -> Self {
    Self { result: Err(Arc::new(error)), error_type: Some(core::any::type_name::<E>()) }
  }

  /// Creates a reply from `result`, keeping only the text of an error.
  ///
  /// Mirrors Pekko's `StatusReply.fromTry`: `Ok` becomes a success, and `Err`
  /// becomes an [ErrorMessage] with the error's `Display` text. Pekko uses the
  /// exception's class name when its message is `null`; a Rust error has no
  /// `null` text, so an empty text is replaced with
  /// [`core::any::type_name`] of `E`. That name is the compiler's rendering
  /// and is not a stable string.
  ///
  /// Pekko's Java API `fromCallable` is this function applied to the result
  /// of a closure that returns `Result`.
  pub fn from_result<T, E>(result: Result<T, E>) -> Self
  where
    T: Any + Send + Sync,
    E: Error,
  {
    match result {
      | Ok(value) => Self::success(value),
      | Err(error) => {
        let text = error.to_string();
        if text.is_empty() {
          Self::error_message(core::any::type_name::<E>())
        } else {
          Self::error_message(text)
        }
      },
    }
  }

  /// Creates a reply from `result`, keeping an error and its concrete type.
  ///
  /// Mirrors Pekko's `StatusReply.fromTryKeepException`. Pekko's Java API
  /// `fromCallableKeepException` is this function applied to the result of a
  /// closure that returns `Result`.
  pub fn from_result_keep_error<T, E>(result: Result<T, E>) -> Self
  where
    T: Any + Send + Sync,
    E: Error + Send + Sync + 'static,
  {
    match result {
      | Ok(value) => Self::success(value),
      | Err(error) => Self::error(error),
    }
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

impl fmt::Debug for StatusReply {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match &self.result {
      | Ok(value) => write!(formatter, "Success({value:?})"),
      | Err(error) if error.downcast_ref::<ErrorMessage>().is_some() => write!(formatter, "Error({error})"),
      | Err(error) => match self.error_type {
        | Some(error_type) => write!(formatter, "Error({error_type}: {error:?})"),
        | None => write!(formatter, "Error({error:?})"),
      },
    }
  }
}
