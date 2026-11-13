// Ported from Pekko v1.7.1:
//   testkit/src/test/scala/org/apache/pekko/testkit/TestEventListenerSpec.scala
//
// Original test names are kept as comments next to each ported case.

use core::any::Any;
use core::fmt;

use fraktor_actor::kernel::actor::Logging;
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;

const SPEC_SYSTEM_NAME: &str = "TestEventListenerSpec";

// The original defines `AnError` as a private class inside the spec. The port
// carries it as a value that records its name, because a Rust filter selects an
// error by that name instead of by a runtime class.
struct AnError;

impl AnError {
  const NAME: &'static str = "AnError";

  fn new() -> Self {
    Self
  }
}

impl fmt::Debug for AnError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", Self::NAME)
  }
}

impl fmt::Display for AnError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", Self::NAME)
  }
}

// The original defines `AnotherError` as a local object that extends
// `Exception`, unrelated to `AnError`.
struct AnotherError;

impl AnotherError {
  const NAME: &'static str = "AnotherError";
}

impl fmt::Debug for AnotherError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", Self::NAME)
  }
}

impl fmt::Display for AnotherError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", Self::NAME)
  }
}

struct TestEventListenerSpecCase {
  fraktor_spec: FraktorSpec,
}

impl TestEventListenerSpecCase {
  fn new() -> Self {
    Self { fraktor_spec: FraktorSpec::new(SPEC_SYSTEM_NAME) }
  }

  // `private def errorNoCause = Error(self.path.toString, this.getClass, "this is an error")`
  fn error_no_cause(&self) -> Logging::Error {
    Logging::Error::new(self.log_source(), 3, "this is an error")
  }

  // `private def errorWithCause(cause) = Error(cause, self.path.toString, this.getClass, "this is an error")`
  fn error_with_cause<C>(&self, cause: C) -> Logging::Error
  where
    C: Any + fmt::Debug + 'static,
  {
    Logging::Error::with_cause(cause, self.log_source(), 3, "this is an error")
  }

  // `private def warningNoCause = Warning(self.path.toString, this.getClass, "this is a warning")`
  fn warning_no_cause(&self) -> Logging::Warning {
    Logging::Warning::new(self.log_source(), 3, "this is a warning")
  }

  // `private def warningWithCause(cause) = Warning(cause, self.path.toString, this.getClass, "this is a warning", Logging.emptyMDC)`
  fn warning_with_cause<C>(&self, cause: C) -> Logging::Warning
  where
    C: Any + fmt::Debug + 'static,
  {
    Logging::Warning::with_cause(cause, self.log_source(), 3, "this is a warning", Logging::empty_mdc())
  }

  // `self.path.toString`, where `self` is the implicit sender (the test actor).
  // The port also names the log class of the original, `this.getClass`, as 3.
  fn log_source(&self) -> String {
    ImplicitSender::self_actor_ref(&self.fraktor_spec).path().to_string()
  }
}

impl Drop for TestEventListenerSpecCase {
  fn drop(&mut self) {
    self.fraktor_spec.terminate();
  }
}

// "The classic EventFilter.error" must { "filter errors without cause" in { ... } }
#[test]
fn classic_error_filter_errors_without_cause() {
  let spec = TestEventListenerSpecCase::new();

  let filter = EventFilter::error();

  assert!(filter.apply(&spec.error_no_cause()));
}

// "The classic EventFilter.error" must { "filter errors with cause" in { ... } }
#[test]
fn classic_error_filter_errors_with_cause() {
  let spec = TestEventListenerSpecCase::new();

  let filter = EventFilter::error();

  assert!(filter.apply(&spec.error_with_cause(AnError::new())));
}

// "The typed EventFilter" must { "not filter errors without cause" in { ... } }
#[test]
fn typed_filter_does_not_filter_errors_without_cause() {
  let spec = TestEventListenerSpecCase::new();

  let filter = EventFilter::typed(AnError::NAME);

  assert!(!filter.apply(&spec.error_no_cause()));
}

// "The typed EventFilter" must { "not filter errors with an unrelated cause" in { ... } }
#[test]
fn typed_filter_does_not_filter_errors_with_unrelated_cause() {
  let spec = TestEventListenerSpecCase::new();

  let filter = EventFilter::typed(AnError::NAME);

  assert!(!filter.apply(&spec.error_with_cause(AnotherError)));
}

// "The typed EventFilter" must { "filter errors with a matching cause" in { ... } }
#[test]
fn typed_filter_filters_errors_with_matching_cause() {
  let spec = TestEventListenerSpecCase::new();

  let filter = EventFilter::typed(AnError::NAME);

  assert!(filter.apply(&spec.error_with_cause(AnError::new())));
}

// "The EventFilter[NoCause]" must { "filter errors without cause" in { ... } }
#[test]
fn no_cause_filter_filters_errors_without_cause() {
  let spec = TestEventListenerSpecCase::new();

  let filter = EventFilter::typed(Logging::Error::NO_CAUSE_NAME);

  assert!(filter.apply(&spec.error_no_cause()));
}

// "The EventFilter[NoCause]" must { "filter errors with (any) cause" in { ... } }
//
// The original keeps the current behaviour: the `NoCause` filter also matches
// events that do carry a cause. See the comment at TestEventListenerSpec.scala:59.
#[test]
fn no_cause_filter_filters_errors_with_any_cause() {
  let spec = TestEventListenerSpecCase::new();

  let filter = EventFilter::typed(Logging::Error::NO_CAUSE_NAME);

  assert!(filter.apply(&spec.error_with_cause(AnError::new())));
}

// "The classic EventFilter.warning" must { "filter warnings without cause" in { ... } }
#[test]
fn classic_warning_filter_warnings_without_cause() {
  let spec = TestEventListenerSpecCase::new();

  let filter = EventFilter::warning();

  assert!(filter.apply(&spec.warning_no_cause()));
}

// "The classic EventFilter.warning" must { "filter warning with cause" in { ... } }
#[test]
fn classic_warning_filter_warning_with_cause() {
  let spec = TestEventListenerSpecCase::new();

  let filter = EventFilter::warning();

  assert!(filter.apply(&spec.warning_with_cause(AnError::new())));
}
