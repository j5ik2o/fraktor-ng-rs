// The four ported cases in `logging_adapter_test.rs` drive the adapter with an
// argument count that matches the `{}` placeholders in every template. The
// formatting entry the earlier milestone reads (`formatImpl` in Pekko's
// `Logging.scala`) has two branches those cases never reach:
//   * fewer arguments than placeholders leaves the unmatched `{}` in place
//     rather than panicking;
//   * more arguments than placeholders appends ` WARNING arguments left: <n>`
//     with the count of the arguments that were not consumed.
//
// This file checks both branches through the same public publish entry the
// ported cases use, so the delivered event's message is read exactly as a
// public caller would read it. The branch inputs mirror `formatImpl`:
// every `{}` is filled left to right, the substitution text is never scanned
// again, and the extra-argument warning follows the formatted text.

use std::sync::Arc;
use std::time::Duration;

use fraktor_actor::kernel::actor::Logging::{Debug, Error, LogClass, LogEvent, LogLevel, LogPublisher};
use fraktor_actor::kernel::event::{EventClass, EventStream};
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_CLASS: &str = "org.apache.pekko.event.LoggingAdapterTest";
const SPEC_LOG_CLASS: i32 = 0;

#[test]
fn missing_argument_keeps_the_unmatched_placeholder() {
  let mut spec = FraktorSpec::new("LogPublisherFormat");
  let mut probe = TestProbe::new(spec.system_mut());
  let stream = spec.system().event_stream();
  assert!(stream.subscribe(Some(probe.ref_handle()), EventClass::of::<Debug>()).expect("the probe actor is not null"));
  let log = LogPublisher::new(&stream, SPEC_CLASS, LogClass::new(SPEC_LOG_CLASS));

  // Two placeholders, one argument: the first `{}` is filled and the second
  // stays as written, so the caller does not have to supply an argument for
  // every placeholder.
  log.log(LogLevel::Debug, "First {}, second {}", &[&"one"]);
  spec.system_mut().run_until_idle();
  let debug: Debug = probe.expect_msg_type();
  assert_eq!(debug.message(), "First one, second {}");

  spec.terminate();
}

#[test]
fn extra_arguments_append_a_warning_with_the_leftover_count() {
  let mut spec = FraktorSpec::new("LogPublisherFormat");
  let mut probe = TestProbe::new(spec.system_mut());
  let stream = spec.system().event_stream();
  assert!(stream.subscribe(Some(probe.ref_handle()), EventClass::of::<Debug>()).expect("the probe actor is not null"));
  let log = LogPublisher::new(&stream, SPEC_CLASS, LogClass::new(SPEC_LOG_CLASS));

  // One placeholder, three arguments: the first fills the placeholder, the
  // other two are counted and reported after the text.
  log.log(LogLevel::Debug, "Only {}", &[&1, &2, &3]);
  spec.system_mut().run_until_idle();
  let debug: Debug = probe.expect_msg_type();
  assert_eq!(debug.message(), "Only 1 WARNING arguments left: 2");

  spec.terminate();
}

#[test]
fn empty_arguments_keep_the_placeholders_as_written() {
  let mut spec = FraktorSpec::new("LogPublisherFormat");
  let mut probe = TestProbe::new(spec.system_mut());
  let stream = spec.system().event_stream();
  assert!(stream.subscribe(Some(probe.ref_handle()), EventClass::of::<Debug>()).expect("the probe actor is not null"));
  let log = LogPublisher::new(&stream, SPEC_CLASS, LogClass::new(SPEC_LOG_CLASS));

  // No arguments: every placeholder stays as written.
  log.log(LogLevel::Debug, "Value {}", &[] as &[&str]);
  spec.system_mut().run_until_idle();
  let debug: Debug = probe.expect_msg_type();
  assert_eq!(debug.message(), "Value {}");

  spec.terminate();
}

#[test]
fn independent_stream_delivers_only_while_its_owner_is_alive() {
  let mut spec = FraktorSpec::new("LogPublisherOwnership");
  let mut probe = TestProbe::new(spec.system_mut());
  let stream = EventStream::new(spec.system().downgrade(), false);
  for class in [EventClass::of::<Debug>(), EventClass::of::<Error>()] {
    assert!(stream.subscribe(Some(probe.ref_handle()), class).expect("the probe actor is not null"));
  }
  let log = LogPublisher::new(&stream, SPEC_CLASS, LogClass::new(SPEC_LOG_CLASS));
  let cause = Arc::new(String::from("owned cause"));

  log.log(LogLevel::Debug, "owned {}", &[7_i32]);
  log.error(cause.clone(), "owned {}", &[7_i32]);
  spec.system_mut().run_until_idle();

  let debug: Debug = probe.expect_msg_type();
  assert_eq!(debug.message(), "owned 7");
  assert_eq!(debug.level(), LogLevel::Debug);
  assert!(debug.mdc().is_empty());
  let error: Error = probe.expect_msg_type();
  assert_eq!(error.message(), "owned 7");
  assert_eq!(error.level(), LogLevel::Error);
  assert!(error.mdc().is_empty());
  let received_cause = error.cause().expect("the error carries the sent cause");
  let received_cause = received_cause.downcast::<String>().expect("the cause keeps its type");
  assert!(Arc::ptr_eq(&cause, &received_cause));
  probe.expect_no_message(Duration::ZERO);

  drop(stream);
  log.log(LogLevel::Debug, "owned {}", &[7_i32]);
  log.error(cause.clone(), "owned {}", &[7_i32]);
  spec.system_mut().run_until_idle();
  probe.expect_no_message(Duration::ZERO);

  spec.terminate();
}
