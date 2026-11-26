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

//! Port of Pekko v2.0.0's classic StatusReplySpec (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f).
//!
//! Upstream: `actor-tests/src/test/scala/org/apache/pekko/pattern/StatusReplySpec.scala`
//! (<https://raw.githubusercontent.com/apache/pekko/v2.0.0/actor-tests/src/test/scala/org/apache/pekko/pattern/StatusReplySpec.scala>).
//!
//! First ported from v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff), which
//! had nine cases. v2.0.0 has twenty, and this file ports all of them. The new
//! ones are "include exception type in toString for non text-error" (upstream
//! 6287b9de07, #3271), the five "create from Try ..." cases (upstream f1a36ba76b,
//! #2796) and the five "create from Callable ..." cases (upstream 037b88ad29,
//! #2812).
//!
//! Scala's `Try[T]` is Rust's `Result<T, E>`: `StatusReply.fromTry` is
//! `StatusReply::from_result` and `fromTryKeepException` is
//! `from_result_keep_error`. Java's `Callable` reports a failure by throwing;
//! the Rust counterpart is a closure that returns `Result<T, E>`. The port has
//! no separate `fromCallable`: each Callable case calls its closure and passes
//! the result to the same two functions. A panic is not caught, because a panic
//! is not a recoverable error in Rust, while Pekko catches only `NonFatal`
//! exceptions.
//!
//! Pekko replaces a `null` exception message with the class name
//! (`java.lang.RuntimeException`). A Rust error always has a `Display` text, and
//! no text is the empty string, so the port replaces an empty text with
//! `core::any::type_name` of the error type. Java distinguishes `null` from an
//! empty message (an empty one is kept); the port cannot, so an empty text is
//! treated as Java's `null`. The type name is the compiler's rendering and is
//! not a stable string, so the cases compare it with `type_name` itself.

use core::error::Error;
use core::fmt;
use core::future::Future;
use core::pin::pin;
use core::task::Context;
use core::task::Poll;
use core::task::Waker;
use core::time::Duration;
use std::sync::Arc;

use fraktor_actor::adaptor::ErrorMessage;
use fraktor_actor::adaptor::StatusAskError;
use fraktor_actor::adaptor::StatusReply;
use fraktor_actor::adaptor::StatusReplyAck;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::timeout::Timeout;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const ASK_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, PartialEq)]
struct TestException(&'static str);

impl fmt::Display for TestException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.0)
  }
}

impl Error for TestException {}

// Stands in for the original's `new RuntimeException()`, whose message is `null`.
#[derive(Debug)]
struct RuntimeException;

impl fmt::Display for RuntimeException {
  fn fmt(&self, _formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    Ok(())
  }
}

impl Error for RuntimeException {}

fn assert_success_woho(reply: &StatusReply) {
  assert!(reply.is_success());
  assert_eq!(reply.get_value().expect("successful reply").downcast_ref::<&'static str>(), Some(&"woho"));
}

fn assert_text_error(reply: &StatusReply, message: &str) {
  assert!(reply.is_error());
  let error = reply.get_error().expect("failed reply");
  assert_eq!(error.to_string(), message);
  // `reply.getError shouldBe a[StatusReply.ErrorMessage]`
  assert!(error.downcast_ref::<ErrorMessage>().is_some(), "the error is an ErrorMessage");
}

fn assert_kept_test_exception(reply: &StatusReply) {
  assert!(reply.is_error());
  // `reply.getError should ===(TestException("boo"))`
  assert_eq!(reply.get_error().expect("failed reply").downcast_ref::<TestException>(), Some(&TestException("boo")));
}

// pattern match success
#[test]
fn success_payload_downcasts_to_the_stored_string_type_and_rejects_int() {
  let reply = StatusReply::success("woho!");
  let message = StatusReply::match_reply(Some(&reply)).expect("present reply").expect("successful reply");

  assert!(message.downcast_ref::<i32>().is_none(), "a String payload must not match Int");
  let text = message.downcast_ref::<&'static str>().expect("stored payload is a string");
  assert_eq!(*text, "woho!");
}

// pattern match success (Ack)
#[test]
fn ack_success_payload_downcasts_to_the_ack_unit_struct() {
  let reply = StatusReply::ack();
  let message = reply.get_value().expect("ack reply is a success");

  assert_eq!(message.downcast_ref::<StatusReplyAck>(), Some(&StatusReplyAck));
}

// not throw exception if null
#[test]
fn null_reply_is_neither_success_nor_error() {
  assert!(StatusReply::match_reply(None).is_none(), "an absent reply must match neither outcome");
}

// pattern match error with text
#[test]
fn text_error_reply_carries_a_concrete_error_message() {
  let reply = StatusReply::error_message("boho!");

  assert!(reply.is_error());
  assert!(!reply.is_success());
  let error = StatusReply::match_reply(Some(&reply)).expect("present reply").expect_err("failed reply");
  assert_eq!(error.downcast_ref::<ErrorMessage>().expect("text error").to_string(), "boho!");
}

// pattern match error with exception
#[test]
fn typed_error_reply_carries_the_original_exception() {
  let reply = StatusReply::error(TestException("boho!"));

  assert!(reply.is_error());
  assert!(!reply.is_success());
  let error = StatusReply::match_reply(Some(&reply)).expect("present reply").expect_err("failed reply");
  assert_eq!(error.downcast_ref::<TestException>().expect("original error").to_string(), "boho!");
}

// include exception type in toString for non text-error
//
// Added in v2.0.0 (upstream 6287b9de07, #3271). Pekko's `toString` renders a
// non-text error as `Error(<exception>)` and leaves the type to the exception's
// own `toString`; `Throwable.toString` starts with the class name, as the
// original's `TestException` does, but an exception may override it. The port
// names the type itself: the reply's `Debug` (it has no `Display`, because a
// success carries a type-erased `Message` that has no text) adds the error's
// type name in front of the error's own `Debug`. The error here prints only
// its message, so the type name can come only from the reply.
#[test]
fn include_exception_type_in_to_string_for_non_text_error() {
  let rendered = format!("{:?}", StatusReply::error(UnnamedTestException("boho!")));
  assert!(rendered.contains("UnnamedTestException"), "rendered [{rendered}]");
  assert!(rendered.contains("boho!"), "rendered [{rendered}]");

  // A text error is rendered as its text, as Pekko's `Error(<message>)`.
  let rendered = format!("{:?}", StatusReply::error_message("boho!"));
  assert_eq!(rendered, "Error(boho!)");
}

// An error whose `Debug` and `Display` print only the message, not the type.
struct UnnamedTestException(&'static str);

impl fmt::Debug for UnnamedTestException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.0)
  }
}

impl fmt::Display for UnnamedTestException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.0)
  }
}

impl Error for UnnamedTestException {}

// flatten a Future[StatusReply]
#[test]
fn flatten_status_future_unwraps_value_ack_and_both_error_kinds() {
  let value = poll_ready(StatusReply::flatten_status_future(core::future::ready(Ok(StatusReply::success("woho")))))
    .expect("flattened success");
  assert_eq!(value.downcast_ref::<&'static str>(), Some(&"woho"));

  let ack =
    poll_ready(StatusReply::flatten_status_future(core::future::ready(Ok(StatusReply::ack())))).expect("flattened ack");
  assert_eq!(ack.downcast_ref::<StatusReplyAck>(), Some(&StatusReplyAck));

  let text_error =
    poll_ready(StatusReply::flatten_status_future(core::future::ready(Ok(StatusReply::error_message("boo")))))
      .expect_err("flattened text error");
  assert_eq!(text_error.downcast_ref::<ErrorMessage>().expect("text error").to_string(), "boo");

  let typed_error =
    poll_ready(StatusReply::flatten_status_future(core::future::ready(Ok(StatusReply::error(TestException("boo"))))))
      .expect_err("flattened typed error");
  assert_eq!(typed_error.downcast_ref::<TestException>().expect("original error").to_string(), "boo");

  let delivery_error: Arc<dyn Error + Send + Sync> = Arc::new(TestException("delivery failed"));
  let error = poll_ready(StatusReply::flatten_status_future(core::future::ready(Err(Arc::clone(&delivery_error)))))
    .expect_err("the future failed before returning a reply");
  assert!(Arc::ptr_eq(&error, &delivery_error));
}

// askWithStatus unwrap success
#[test]
fn ask_with_status_unwraps_success_value() {
  let result = ask_probe(StatusReply::success("woho"));

  let value = result.expect("successful ask");
  assert_eq!(value.downcast_ref::<&'static str>(), Some(&"woho"));
}

// askWithStatus unwrap Error with message
#[test]
fn ask_with_status_unwraps_text_error() {
  let error = ask_probe(StatusReply::error_message("boho")).expect_err("text reply must fail the ask");
  let StatusAskError::Reply(error) = error else {
    panic!("expected a reply error, got {error:?}");
  };
  assert_eq!(error.downcast_ref::<ErrorMessage>().expect("text error").to_string(), "boho");
}

// askWithStatus unwrap Error with exception
#[test]
fn ask_with_status_unwraps_original_exception() {
  let error = ask_probe(StatusReply::error(TestException("boho"))).expect_err("typed reply must fail the ask");
  let StatusAskError::Reply(error) = error else {
    panic!("expected a reply error, got {error:?}");
  };
  assert_eq!(error.downcast_ref::<TestException>().expect("original error").to_string(), "boho");
}

// create from Try success
#[test]
fn create_from_try_success() {
  let reply = StatusReply::from_result(Ok::<_, TestException>("woho"));
  assert_success_woho(&reply);
}

// create from Try failure with message
#[test]
fn create_from_try_failure_with_message() {
  let reply = StatusReply::from_result(Err::<&'static str, _>(TestException("boo")));
  assert_text_error(&reply, "boo");
}

// create from Try failure keeping exception
#[test]
fn create_from_try_failure_keeping_exception() {
  let reply = StatusReply::from_result_keep_error(Err::<&'static str, _>(TestException("boo")));
  assert_kept_test_exception(&reply);
}

// create from Try success via fromTryKeepException
#[test]
fn create_from_try_success_via_from_try_keep_exception() {
  let reply = StatusReply::from_result_keep_error(Ok::<_, TestException>("woho"));
  assert_success_woho(&reply);
}

// create from Try failure with null message
#[test]
fn create_from_try_failure_with_null_message() {
  let reply = StatusReply::from_result(Err::<&'static str, _>(RuntimeException));
  assert!(reply.is_error());
  assert_eq!(reply.get_error().expect("failed reply").to_string(), core::any::type_name::<RuntimeException>());
}

// create from Callable success
#[test]
fn create_from_callable_success() {
  let callable = || -> Result<&'static str, TestException> { Ok("woho") };
  let reply = StatusReply::from_result(callable());
  assert_success_woho(&reply);
}

// create from Callable failure with message
#[test]
fn create_from_callable_failure_with_message() {
  let callable = || -> Result<&'static str, TestException> { Err(TestException("boo")) };
  let reply = StatusReply::from_result(callable());
  assert_text_error(&reply, "boo");
}

// create from CallableKeepException success
#[test]
fn create_from_callable_keep_exception_success() {
  let callable = || -> Result<&'static str, TestException> { Ok("woho") };
  let reply = StatusReply::from_result_keep_error(callable());
  assert_success_woho(&reply);
}

// create from CallableKeepException failure keeping exception
#[test]
fn create_from_callable_keep_exception_failure_keeping_exception() {
  let callable = || -> Result<&'static str, TestException> { Err(TestException("boo")) };
  let reply = StatusReply::from_result_keep_error(callable());
  assert_kept_test_exception(&reply);
}

// create from Callable failure with null message
#[test]
fn create_from_callable_failure_with_null_message() {
  let callable = || -> Result<&'static str, RuntimeException> { Err(RuntimeException) };
  let reply = StatusReply::from_result(callable());
  assert!(reply.is_error());
  assert_eq!(reply.get_error().expect("failed reply").to_string(), core::any::type_name::<RuntimeException>());
}

fn poll_ready<F: Future>(future: F) -> F::Output {
  let mut future = pin!(future);
  let waker = Waker::noop();
  let mut context = Context::from_waker(waker);
  match future.as_mut().poll(&mut context) {
    | Poll::Ready(output) => output,
    | Poll::Pending => panic!("a future that is ready must complete on the first poll"),
  }
}

fn ask_probe(reply: StatusReply) -> Result<Message, StatusAskError> {
  let mut spec = FraktorSpec::new("StatusReplySpec");
  let mut probe = TestProbe::new(spec.system_mut());
  let result = probe.ref_handle().ask_with_status("request", Timeout::new(ASK_TIMEOUT));
  probe.expect_msg("request");
  probe.last_message().sender().tell(reply, spec.system().dead_letters());
  let result = result.try_await_result(ASK_TIMEOUT);
  spec.terminate();
  result
}
