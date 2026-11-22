// Ported from Pekko commit 9a54d1294c3ac7821bbed3c940769c3d5027d1ff:
//   actor-tests/src/test/java/org/apache/pekko/event/LoggingAdapterTest.java
//   actor-tests/src/test/java/org/apache/pekko/event/ActorWithMDC.java
//
// The original holds one suite, `LoggingAdapterTest`, with four cases. The port
// keeps all four cases one for one and keeps the original case names as the
// function names above each test. Each case subscribes a `TestProbe` to the
// event classes of the system's own event stream, drives the log through the
// production entry point, and reads the delivered event back from that probe.
//
// The port needs two things that the original gets from the logging adapter and
// that this crate does not have yet:
//   * a logging adapter built from an event stream and a log source, mirroring
//     `Logging.getLogger(system, this)`, that publishes a formatted event at one
//     level through a single entry `log(level, message, args)`, that has one
//     cause-carrying entry `log_with_cause(level, cause, message, args)`, and
//     that holds a diagnostic context through `set_mdc` / `clear_mdc`, mirroring
//     `DiagnosticLoggingAdapter`. The adapter must be `Send + Sync`, because an
//     actor is started from a `Props` factory that is `Send + Sync` and holds the
//     adapter;
//   * an error event that exposes the cause that the adapter published, so the
//     ported cause case can compare the received cause with the sent one through
//     `Arc::ptr_eq`, mirroring `LogEventWithCause.cause()` and the original's
//     `assertSame`.
// The port asks for them by name (`LoggingAdapter`, `LoggingAdapter::log`,
// `LoggingAdapter::log_with_cause`, `LoggingAdapter::set_mdc`,
// `LoggingAdapter::clear_mdc`, `Error::cause`) and does not implement them here,
// because this milestone only ports the tests; a missing entry is the expected
// red, as it was for the marker logging milestone.
//
// The original `LoggingAdapter` mixes arity overloads (`error(message)` and
// `error(template, arg)`). Rust has no same-name arity overloads, so the port
// uses one signature for every level and every argument count: the level is the
// first argument, the template the second, and the arguments one slice of
// `&dyn Display` the third, so an empty slice is the plain message. The mirror of
// Java's `Object[]` arguments is therefore `&[&dyn Display]`; a Java `int[]`
// argument becomes the same slice of borrowed integers, which keeps the
// primitive-array case distinct from the heterogeneous-object case.
//
// Differences from the original that need a note:
//   * The original subscribes to `LogEvent.class`, the common parent of every
//     log event. The port subscribes to the concrete class of each expected
//     event, because the port's stream matches a subscription by the exact
//     event type and does not widen to a parent; the ported cases expect the
//     same events in the same order, so the difference is not observable.
//   * The original reads the MDC with `log.getMDC().toString()`, which renders
//     a Scala map as `{messageLength=8}` and an empty map as `{}`. The port
//     keeps the MDC as a `BTreeMap`, so it compares the map itself: an empty
//     map is `Mdc::new()`, and a non-empty one holds `messageLength` as its one
//     entry. Pekko stores the `Int` length; the port's MDC is a
//     `BTreeMap<String, String>`, so the length is the string form of the same
//     value, and `"8"`, `"9"`, `"9"`, `"29"` stand for the original's `8`, `9`,
//     `9`, `29`.
//   * The original passes the `Date` and the `UUID` and expects their
//     `toString()`. The port passes values whose `Display` is the text the
//     original reads; it keeps the argument types distinct (a string, a time
//     text that stands in for the `Date`, and an identifier text that stands in
//     for the `UUID`) so the case still sends a heterogeneous array rather than
//     an array of strings, and it uses fixed values instead of the clock and
//     the random generator.
//   * The original's `LoggingAdapterTest.class` is a Java class object; the port
//     passes its name as the log source and does not assert on the log class,
//     which no case of the original reads.

use core::fmt;

use std::sync::Arc;

use fraktor_actor::kernel::actor::Logging::{
  empty_mdc, Debug, Error, Info, LogEvent, LogLevel, LoggingAdapter, Mdc, Warning,
};
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

// The original passes `this.getClass` as the log source; the port keeps the
// suite's fully qualified name, the same name the marker logging milestone uses
// for its suite.
const SPEC_CLASS: &str = "org.apache.pekko.event.LoggingAdapterTest";

// The original's `IllegalStateException` with the stack trace suppressed. The
// port keeps distinct `Debug` and `Display` texts so the case can tell what the
// event keeps as the cause name from what it keeps as the cause message.
struct IllegalStateException {
  message: String,
}

impl IllegalStateException {
  const NAME: &'static str = "IllegalStateException";

  fn new(message: &str) -> Self {
    Self { message: message.to_string() }
  }
}

impl fmt::Debug for IllegalStateException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", IllegalStateException::NAME)
  }
}

impl fmt::Display for IllegalStateException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", self.message)
  }
}

// The original passes `new Exception()` to the old `Error` constructor. A plain
// `Exception` has no message, so the only fact the event reads is the class
// name, which the port keeps through `Debug`. The type has no `Display`, which
// keeps the case on the existing `Error::with_cause` bound.
struct PlainException;

impl PlainException {
  const NAME: &'static str = "Exception";
}

impl fmt::Debug for PlainException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", PlainException::NAME)
  }
}

// The original's `java.util.Date` argument. It stands in for `new Date()`, or,
// on JDK 9 and later, for the `StringBuilder`-based `toString` the original
// reads; the case only needs a value whose `Display` is the text it expects, so
// the port keeps the text fixed and does not read the clock.
struct JavaDate;

impl fmt::Display for JavaDate {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "Sun Feb 10 16:02:20 UTC 2019")
  }
}

// The original's `java.util.UUID` argument. It stands in for
// `UUID.randomUUID()`; the case only reads the identifier's `toString`, so the
// port keeps the text fixed.
struct JavaUuid;

impl fmt::Display for JavaUuid {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "4192af46-487c-4132-84e8-87890cbb00a6")
  }
}

// "LoggingAdapterTest" must { "mustFormatMessage" in { ... } }
#[test]
fn must_format_message() {
  let mut spec = FraktorSpec::new("LoggingAdapterTest");
  let mut probe = TestProbe::new(spec.system_mut());
  let stream = spec.system().event_stream();
  // `system.getEventStream().subscribe(getRef(), LogEvent.class)`. The port
  // subscribes the classes of the four events the case expects; the case drives
  // one level per call, so no other event reaches the probe.
  for class in
    [EventClass::of::<Error>(), EventClass::of::<Warning>(), EventClass::of::<Info>(), EventClass::of::<Debug>()]
  {
    assert!(stream.subscribe(Some(probe.ref_handle()), class).expect("the probe actor is not null"));
  }
  let log = LoggingAdapter::new(spec.system().event_stream(), SPEC_CLASS);

  // `log.error("One arg message: {}", "the arg")`
  log.log(LogLevel::Error, "One arg message: {}", &[&"the arg"]);
  spec.system_mut().run_until_idle();
  let error: Error = probe.expect_msg_type();
  assert_eq!(error.level(), LogLevel::Error);
  assert_eq!(error.message(), "One arg message: the arg");
  assert_eq!(error.mdc(), &empty_mdc());

  // `log.error(cause, "Two args message: {}, {}", "an arg", "another arg")`
  let cause = Arc::new(IllegalStateException::new("This state is illegal"));
  log.log_with_cause(LogLevel::Error, cause.clone(), "Two args message: {}, {}", &[&"an arg", &"another arg"]);
  spec.system_mut().run_until_idle();
  let error: Error = probe.expect_msg_type();
  assert_eq!(error.level(), LogLevel::Error);
  assert_eq!(error.message(), "Two args message: an arg, another arg");
  assert_eq!(error.mdc(), &empty_mdc());
  // `assertSame(cause, causedEvent.cause())`: the delivered event carries the
  // cause the case sent, not a copy of its name or its text.
  let received_cause = error.cause().expect("the error carries the cause the call sent");
  let received_cause = received_cause.downcast::<IllegalStateException>().expect("the cause keeps its type");
  assert!(Arc::ptr_eq(&cause, &received_cause), "the delivered cause is the sent cause");

  // `int[] primitiveArgs = {10, 20, 30}; log.warning(..., primitiveArgs)`
  let primitive_args: [&dyn fmt::Display; 3] = [&10, &20, &30];
  log.log(LogLevel::Warning, "Args as array of primitives: {}, {}, {}", &primitive_args);
  spec.system_mut().run_until_idle();
  let warning: Warning = probe.expect_msg_type();
  assert_eq!(warning.level(), LogLevel::Warning);
  assert_eq!(warning.message(), "Args as array of primitives: 10, 20, 30");
  assert_eq!(warning.mdc(), &empty_mdc());

  // `Object[] objArgs = {"A String", now, uuid}; log.info(..., objArgs)`
  let object_args: [&dyn fmt::Display; 3] = [&"A String", &JavaDate, &JavaUuid];
  log.log(LogLevel::Info, "Args as array of objects: {}, {}, {}", &object_args);
  spec.system_mut().run_until_idle();
  let info: Info = probe.expect_msg_type();
  assert_eq!(info.level(), LogLevel::Info);
  assert_eq!(
    info.message(),
    "Args as array of objects: A String, Sun Feb 10 16:02:20 UTC 2019, 4192af46-487c-4132-84e8-87890cbb00a6"
  );
  assert_eq!(info.mdc(), &empty_mdc());

  // `log.debug("Four args message: {}, {}, {}, {}", 1, 2, 3, 4)`
  log.log(LogLevel::Debug, "Four args message: {}, {}, {}, {}", &[&1, &2, &3, &4]);
  spec.system_mut().run_until_idle();
  let debug: Debug = probe.expect_msg_type();
  assert_eq!(debug.level(), LogLevel::Debug);
  assert_eq!(debug.message(), "Four args message: 1, 2, 3, 4");
  assert_eq!(debug.mdc(), &empty_mdc());

  spec.terminate();
}

// Original: `ActorWithMDC.Log(level, message)`.
struct Log {
  level: LogLevel,
  message: String,
}

impl Log {
  fn new(level: LogLevel, message: &str) -> Self {
    Self { level, message: message.to_string() }
  }
}

// Returns the MDC the actor sets for a message whose length is `length`, which
// is the one-entry map Pekko renders as `{messageLength=<length>}`.
fn length_mdc(length: &str) -> Mdc {
  let mut mdc = Mdc::new();
  mdc.insert("messageLength".to_string(), length.to_string());
  mdc
}

// Original: `LoggingAdapterTest.ActorWithMDC`. The actor keeps one adapter and
// sets the MDC for each message before it logs, then clears it. The port keeps
// the same decisions: a message that starts with "No MDC" sets an explicit
// empty context, "Null MDC" passes the missing context, and every other message
// records its length; then the actor logs and clears so the next message starts
// empty.
//
// The original reaches two distinct adapter states from its two branches:
// `setMDC(Collections.emptyMap())` for "No MDC" and `setMDC(null)` for "Null
// MDC". The port keeps the same two inputs distinct at the adapter entry: the
// explicit empty context is a present map with no entry, and the missing
// context is `None`. Both must publish an event whose MDC is empty, but the
// case drives them through two different inputs.
//
// The actor is started from `Props` exactly as the original starts
// `system.actorOf(Props.create(ActorWithMDC.class))`. The `Props` factory is
// `Send + Sync`, so it captures a cloned adapter; the adapter is `Send + Sync`
// because it holds a sendable publish handle rather than the event stream
// itself. The actor keeps that adapter across messages and logs through it.
struct ActorWithMdc {
  adapter: LoggingAdapter,
}

impl ActorWithMdc {
  fn new(adapter: LoggingAdapter) -> Self {
    Self { adapter }
  }
}

impl Actor for ActorWithMdc {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    let Some(log) = message.downcast_ref::<Log>() else {
      return;
    };
    if log.message.starts_with("No MDC") {
      // `Collections.emptyMap()`: an explicit empty context.
      self.adapter.set_mdc(Some(&empty_mdc()));
    } else if log.message == "Null MDC" {
      // `null`: no context at all.
      self.adapter.set_mdc(None);
    } else {
      self.adapter.set_mdc(Some(&length_mdc(&log.message.chars().count().to_string())));
    }

    self.adapter.log(log.level, log.message.as_str(), &[]);

    self.adapter.clear_mdc();
  }
}

// "LoggingAdapterTest" must { "mustCallMdcForEveryLog" in { ... } }
#[test]
fn must_call_mdc_for_every_log() {
  let mut spec = FraktorSpec::new("LoggingAdapterTest");
  let mut probe = TestProbe::new(spec.system_mut());
  let stream = spec.system().event_stream();
  for class in
    [EventClass::of::<Error>(), EventClass::of::<Warning>(), EventClass::of::<Info>(), EventClass::of::<Debug>()]
  {
    assert!(stream.subscribe(Some(probe.ref_handle()), class).expect("the probe actor is not null"));
  }
  let adapter = LoggingAdapter::new(spec.system().event_stream(), SPEC_CLASS);
  // `system.actorOf(Props.create(ActorWithMDC.class))`: the factory captures a
  // clone of the adapter, and the child started under the probe sends its
  // events into the probe's queue.
  let ref_handle = probe.child_actor_of(Props::new(move || ActorWithMdc::new(adapter.clone())));

  for (level, message) in [
    (LogLevel::Error, "An Error"),
    (LogLevel::Warning, "A Warning"),
    (LogLevel::Info, "Some Info"),
    (LogLevel::Debug, "No MDC for 4th call"),
    (LogLevel::Debug, "And now yes, a debug with MDC"),
  ] {
    probe.send(&ref_handle, Log::new(level, message));
    spec.system_mut().run_until_idle();
  }

  // The original reads each event with `expectLog(level, message[, mdc])` from
  // the same test actor, in order, and checks the level the event carries. The
  // port reads each event from the same probe with the concrete type of the
  // level that call used, so the order and the level are both checked.
  let an_error: Error = probe.expect_msg_type();
  assert_eq!(an_error.message(), "An Error");
  assert_eq!(an_error.level(), LogLevel::Error);
  assert_eq!(an_error.mdc(), &length_mdc("8"));

  let a_warning: Warning = probe.expect_msg_type();
  assert_eq!(a_warning.message(), "A Warning");
  assert_eq!(a_warning.level(), LogLevel::Warning);
  assert_eq!(a_warning.mdc(), &length_mdc("9"));

  let some_info: Info = probe.expect_msg_type();
  assert_eq!(some_info.message(), "Some Info");
  assert_eq!(some_info.level(), LogLevel::Info);
  assert_eq!(some_info.mdc(), &length_mdc("9"));

  let no_mdc: Debug = probe.expect_msg_type();
  assert_eq!(no_mdc.message(), "No MDC for 4th call");
  assert_eq!(no_mdc.level(), LogLevel::Debug);
  assert_eq!(no_mdc.mdc(), &empty_mdc());

  let with_mdc: Debug = probe.expect_msg_type();
  assert_eq!(with_mdc.message(), "And now yes, a debug with MDC");
  assert_eq!(with_mdc.level(), LogLevel::Debug);
  assert_eq!(with_mdc.mdc(), &length_mdc("29"));

  spec.terminate();
}

// "LoggingAdapterTest" must { "mustSupportMdcNull" in { ... } }
#[test]
fn must_support_mdc_null() {
  let mut spec = FraktorSpec::new("LoggingAdapterTest");
  let mut probe = TestProbe::new(spec.system_mut());
  let stream = spec.system().event_stream();
  assert!(stream.subscribe(Some(probe.ref_handle()), EventClass::of::<Info>()).expect("the probe actor is not null"));
  let adapter = LoggingAdapter::new(spec.system().event_stream(), SPEC_CLASS);
  let ref_handle = probe.child_actor_of(Props::new(move || ActorWithMdc::new(adapter.clone())));

  probe.send(&ref_handle, Log::new(LogLevel::Info, "Null MDC"));
  spec.system_mut().run_until_idle();

  let info: Info = probe.expect_msg_type();
  assert_eq!(info.message(), "Null MDC");
  assert_eq!(info.level(), LogLevel::Info);
  assert_eq!(info.mdc(), &empty_mdc());

  spec.terminate();
}

// "LoggingAdapterTest" must { "mustBeAbleToCreateLogEventsWithOldConstructor" in { ... } }
#[test]
fn must_be_able_to_create_log_events_with_old_constructor() {
  // `new Error(new Exception(), "logSource", LoggingAdapterTest.class, "The message")`
  // `new Exception()` has no message; the port's cause type prints its class
  // name through `Debug`, which is what `Error::with_cause` keeps as the name.
  let caused = Error::with_cause(PlainException, SPEC_CLASS.to_string(), 0, "The message");
  assert_eq!(caused.message(), "The message");
  assert_eq!(caused.cause_name(), Some(PlainException::NAME));

  // `new Error("logSource", LoggingAdapterTest.class, "The message")`
  let error = Error::new(SPEC_CLASS.to_string(), 0, "The message");
  assert_eq!(error.message(), "The message");
  assert_eq!(error.level(), LogLevel::Error);
  assert_eq!(error.mdc(), &empty_mdc());

  // `new Warning("logSource", LoggingAdapterTest.class, "The message")`
  let warning = Warning::new(SPEC_CLASS.to_string(), 0, "The message");
  assert_eq!(warning.message(), "The message");
  assert_eq!(warning.level(), LogLevel::Warning);
  assert_eq!(warning.mdc(), &empty_mdc());

  // `new Info("logSource", LoggingAdapterTest.class, "The message")`
  let info = Info::new(SPEC_CLASS.to_string(), 0, "The message");
  assert_eq!(info.message(), "The message");
  assert_eq!(info.level(), LogLevel::Info);
  assert_eq!(info.mdc(), &empty_mdc());

  // `new Debug("logSource", LoggingAdapterTest.class, "The message")`
  let debug = Debug::new(SPEC_CLASS.to_string(), 0, "The message");
  assert_eq!(debug.message(), "The message");
  assert_eq!(debug.level(), LogLevel::Debug);
  assert_eq!(debug.mdc(), &empty_mdc());
}
