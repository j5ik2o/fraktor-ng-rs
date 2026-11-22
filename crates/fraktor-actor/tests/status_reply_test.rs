//! Port of the six cases in Pekko v1.7.1's Java classic StatusReplyTest.
//!
//! Upstream: `actor-tests/src/test/java/org/apache/pekko/pattern/StatusReplyTest.java`.

use core::error::Error;
use core::fmt;
use core::time::Duration;

use fraktor_actor::adaptor::ErrorMessage;
use fraktor_actor::adaptor::StatusAskError;
use fraktor_actor::adaptor::StatusReply;
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

// successReplyThrowsExceptionWhenGetErrorIsCalled
#[test]
fn success_reply_returns_value_and_rejects_error_access() {
  let reply = StatusReply::success("woho");

  assert!(reply.is_success());
  assert!(!reply.is_error());
  assert_eq!(reply.get_value().expect("successful reply"), "woho");
  assert!(reply.get_error().is_none());
}

// failedReplyThrowsExceptionWhenGetValueIsCalled
#[test]
fn text_error_reply_returns_error_message_on_value_access() {
  let reply = StatusReply::error_message("boho");

  assert!(reply.is_error());
  assert!(!reply.is_success());
  assert_eq!(reply.get_error().expect("error reply").to_string(), "boho");
  let error = reply.get_value().expect_err("value access must fail");
  assert_eq!(error.downcast_ref::<ErrorMessage>().expect("text error").to_string(), "boho");
}

// failedReplyThrowsOriginalExceptionWhenGetValueIsCalled
#[test]
fn typed_error_reply_returns_original_error_on_value_access() {
  let reply = StatusReply::error(TestException("boho"));

  assert!(reply.is_error());
  assert!(!reply.is_success());
  assert_eq!(reply.get_error().expect("error reply").to_string(), "boho");
  let error = reply.get_value().expect_err("value access must fail");
  assert_eq!(error.downcast_ref::<TestException>().expect("original error").to_string(), "boho");
}

// askWithStatusSuccessReturnsValue
#[test]
fn ask_with_status_success_returns_value() {
  let result = ask_probe(StatusReply::success("woho"));

  assert_eq!(result.expect("successful ask"), "woho");
}

// askWithStatusErrorReturnsErrorMessageExceptionForText
#[test]
fn ask_with_status_returns_text_error_message() {
  let error = ask_probe(StatusReply::error_message("boho")).expect_err("text reply must fail the ask");
  let StatusAskError::Reply(error) = error else {
    panic!("expected a reply error, got {error:?}");
  };
  assert_eq!(error.downcast_ref::<ErrorMessage>().expect("text error").to_string(), "boho");
}

// askWithStatusErrorReturnsOriginalException
#[test]
fn ask_with_status_returns_original_error() {
  let error = ask_probe(StatusReply::error(TestException("boho"))).expect_err("typed reply must fail the ask");
  let StatusAskError::Reply(error) = error else {
    panic!("expected a reply error, got {error:?}");
  };
  assert_eq!(error.downcast_ref::<TestException>().expect("original error").to_string(), "boho");
}

fn ask_probe(reply: StatusReply) -> Result<Message, StatusAskError> {
  let mut spec = FraktorSpec::new("JavaAPI");
  let mut probe = TestProbe::new(spec.system_mut());
  let result = probe.ref_handle().ask_with_status("request", Timeout::new(ASK_TIMEOUT));
  probe.expect_msg("request");
  probe.last_message().sender().tell(reply, spec.system().dead_letters());
  let result = result.try_await_result(ASK_TIMEOUT);
  spec.terminate();
  result
}
