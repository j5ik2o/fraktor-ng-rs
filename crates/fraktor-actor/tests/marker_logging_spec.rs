// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/event/MarkerLoggingSpec.scala
//
// The original holds one suite, "A MarkerLoggerAdapter", with two cases. The
// port keeps both cases one for one and keeps the original case names as the
// comments above each test. Each case subscribes a TestProbe to one event
// class on the actor system's own event stream, publishes the log through the
// marker logging entry, and reads the delivered event back from that probe.
//
// The port needs two things that the original gets from the marker logging
// implementation and that this crate does not have yet:
//   * a marker logging entry, mirroring `new MarkerLoggingAdapter(...)` followed
//     by `info(marker, message)` / `error(marker, cause, message)`;
//   * an info event that carries the `marker` the case reads back, mirroring the
//     `Info3` the original expects for `info(LogMarker.Security, ...)`, and an
//     error event that exposes the cause's `getMessage`, mirroring what the
//     original reads with `cause.getMessage`.
// The port asks for them by name (`LogMarkerAdapter`, `LogMarker`, `Info::marker`,
// `Error::cause_message`) and does not implement them here, because this
// milestone only ports the test; a missing API is the expected red.
//
// Three differences from the original need a note:
//   * Pekko's `TestEvent.Mute(EventFilter)` is a pre-publish step that silences
//     the expected event while the body runs; Pekko's `Info` is not silenced by
//     the port's filter model. The info case therefore omits that mute and reads
//     the message and the marker straight from the event the subscription
//     delivers. The omitted mute is recorded as a known gap, not replaced by a
//     filter the case cannot match: an `EventFilter` in this port has no `Info`
//     variant, so no available filter matches an `Info`.
//   * The original reads the cause message with `cause.getMessage`. The port
//     keeps the rendered cause text on the error event, so the case reads the
//     same text through a method named `cause_message`. It does not read the
//     cause's type name (`cause_name`, which is `MyException`), because a type
//     name is a different fact from the message.
//   * The error case reuses `EventFilter::typed(MyException).with_message("this
//     is a security crash")` and checks it against the actual received `Error`,
//     so the mute the original installs is exercised on the delivered event.

use core::fmt;
use core::time::Duration;

use fraktor_actor::kernel::actor::Logging::{Error, Info, LogClass, LogEvent, LogMarker, LogMarkerAdapter};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

// The original names the suite's class in `getClass.getName` and passes
// `this.getClass` as the log class; the port keeps the name as the log source
// and the class as a `LogClass`, which is the form the marker logging entry
// takes.
const SPEC_CLASS: &str = "org.apache.pekko.event.MarkerLoggingSpec";

// Pekko's argument-less `expectMsgType` uses `single-expect-default`; the port
// spells that wait out.
fn default_timeout() -> Duration {
  Duration::from_secs(3)
}

// The marker logging entry the original builds as
// `new MarkerLoggingAdapter(system.eventStream, getClass.getName, this.getClass, filter)`.
//
// The handle is built from the event stream the subscription already sits on,
// before the case drives the system, so a case never holds `system_mut()` and
// borrows `system()` again from inside a body.
//
// The log class goes through `LogClass` so every `i32` the case passes reaches
// the delivered events unchanged, including the extremes exercised below.
fn marker_logging(system: &ActorSystem, log_class: i32) -> LogMarkerAdapter {
  LogMarkerAdapter::new(system.event_stream(), SPEC_CLASS, LogClass::new(log_class))
}

// The original declares `class MyException(message) extends Exception(message)`.
// The port keeps distinct `Debug` and `Display` texts so the case can tell the
// cause's type name from its Display message.
struct MyException {
  message: String,
}

impl MyException {
  const NAME: &'static str = "MyException";

  fn new(message: &str) -> Self {
    Self { message: message.to_string() }
  }
}

impl fmt::Debug for MyException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", MyException::NAME)
  }
}

impl fmt::Display for MyException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", self.message)
  }
}

// "A MarkerLoggerAdapter" must { "add markers to logging" in { ... } }
#[test]
fn add_markers_to_logging() {
  // The original passes `this.getClass` as the log class. The two ported cases
  // use opposite ends of the `i32` range to check that
  // `LogClass` preserves the class through delivery without repeating a case.
  let mut spec = FraktorSpec::new("MarkerLoggingSpec");
  let mut probe = TestProbe::new(spec.system_mut());
  let stream = spec.system().event_stream();
  assert!(stream.subscribe(Some(probe.ref_handle()), EventClass::of::<Info>()).expect("the probe actor is not null"));
  let logging = marker_logging(spec.system(), i32::MIN);

  // `markerLogging.info(LogMarker.Security, "This is a security problem")`
  //
  // The original also publishes `TestEvent.Mute(EventFilter.info())` before this
  // call; the port has no info filter (see the note at the top), so the case
  // observes the message and the marker from the delivered event instead.
  logging.info(LogMarker::Security, "This is a security problem");
  spec.system_mut().run_until_idle();

  // `val info = expectMsgType[Info3]`
  let info: Info = spec.within(default_timeout(), |_| probe.expect_msg_type());
  assert_eq!(info.message(), "This is a security problem");
  assert_eq!(info.marker().map(|marker| marker.name()), Some("SECURITY"));
  // The `LogClass` reaches the event unchanged, so no value is narrowed.
  assert_eq!(info.log_class(), i32::MIN);

  spec.terminate();
}

// "A MarkerLoggerAdapter" must { "allow cause exceptions in error messages" in { ... } }
#[test]
fn allow_cause_exceptions_in_error_messages() {
  let mut spec = FraktorSpec::new("MarkerLoggingSpec");
  let mut probe = TestProbe::new(spec.system_mut());
  let stream = spec.system().event_stream();
  assert!(stream.subscribe(Some(probe.ref_handle()), EventClass::of::<Error>()).expect("the probe actor is not null"));
  let logging = marker_logging(spec.system(), i32::MAX);

  // `markerLogging.error(LogMarker.Security, new MyException("Security Exception"), "this is a security crash")`
  //
  // The marker is sent with the error, as the original sends `LogMarker.Security`
  // to `error`.
  logging.error(LogMarker::Security, MyException::new("Security Exception"), "this is a security crash");
  spec.system_mut().run_until_idle();

  // `expectMsgType[Error]`
  let error: Error = spec.within(default_timeout(), |_| probe.expect_msg_type());

  // `system.eventStream.publish(TestEvent.Mute(EventFilter[MyException]("this is a security crash")))`
  // The original mutes the expected failure; the port checks the same filter
  // against the delivered event, so the cause type and the exact message agree.
  let filter = EventFilter::typed(MyException::NAME).with_message("this is a security crash");
  assert!(filter.apply(&error), "the typed filter must match the delivered error");

  assert_eq!(error.message(), "this is a security crash");
  // Pekko reads the cause's `getMessage`, which is the cause's Display text. The
  // port must not answer with the cause's type name, which is `MyException`.
  assert_eq!(error.cause_message(), Some("Security Exception"));
  // The error keeps the marker the call sent, as `error(marker, ...)` did.
  assert_eq!(error.marker(), Some(LogMarker::Security));
  // The `LogClass` reaches the error event unchanged, as it does for `info`.
  assert_eq!(error.log_class(), i32::MAX);

  spec.terminate();
}
