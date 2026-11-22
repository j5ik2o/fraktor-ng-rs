//! Port of the nine cases in Pekko v1.7.1's classic StatusReplySpec.
//!
//! Upstream: `actor-tests/src/test/scala/org/apache/pekko/pattern/StatusReplySpec.scala`
//! (<https://raw.githubusercontent.com/apache/pekko/v1.7.1/actor-tests/src/test/scala/org/apache/pekko/pattern/StatusReplySpec.scala>).

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

#[derive(Debug)]
struct TestException(&'static str);

impl fmt::Display for TestException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.0)
  }
}

impl Error for TestException {}

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
