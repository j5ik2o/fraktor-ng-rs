// Reproduction for review thread E1:
//   calling set_log_level before the loggers start leaks the default loggers'
//   subscription.
//
// The bot example (Devin event_stream.rs:233) says: call `set_log_level(Info)`
// before any logger starts, then start the default loggers at `Info`. Because
// `set_log_level` registers only the difference from the saved level, and the
// saved level is already `Info` when the loggers start, the loggers are
// subscribed to no level at all, so every log event (Error included) is lost.
// Pekko's `startDefaultLoggers` subscribes each logger to the channels of the
// configured level at start-up, so an `Info` setting delivers `Info` and every
// more severe level.
//
// This test checks only that claim: an `Error` published after start reaches
// the logger started at `Info`.

use core::time::Duration;

use fraktor_actor::kernel::actor::Logging::{Error, InitializeLogger, LogLevel};
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::EventStream;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;

// A logger that forwards every log event it receives to a destination set at
// start, mirroring the logger of the ported spec.
struct ForwardingLogger {
  destination: ActorRef,
}

impl ForwardingLogger {
  fn new(destination: ActorRef) -> Self {
    Self { destination }
  }
}

impl Actor for ForwardingLogger {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(initialize) = message.downcast_ref::<InitializeLogger>() {
      let bus = initialize.bus();
      let _ = bus;
      context.tell(&context.self_ref(), fraktor_actor::kernel::actor::Logging::LoggerInitialized);
      return;
    }
    if message.is::<Error>() {
      context.tell(&self.destination, message);
    }
  }
}

fn wait() -> Duration {
  Duration::from_secs(1)
}

#[test]
fn g0f_review_e1_start_before_loggers_still_delivers_errors() {
  let mut spec = FraktorSpec::new("G0fReviewE1");
  let bus = EventStream::new(spec.system().downgrade(), false);

  // Set the level before any logger exists.
  bus.set_log_level(LogLevel::Info);

  let destination = spec.test_actor();
  let logger = spec.system_mut().actor_of(Props::new(move || ForwardingLogger::new(destination.clone())));

  // Start the loggers at the same level that was set before them.
  bus.start_default_loggers(&[logger], LogLevel::Info);

  // An error is more severe than `Info`, so a logger started at `Info` must
  // receive it.
  bus.publish(Error::new(String::new(), 0, "boom"));

  spec.within(wait(), |spec| {
    spec.expect_msg_type::<Error>();
  });
  spec.terminate();
}

// Control: the same logger fixture receives the `Error` when the level is not
// set before the loggers start. This isolates the failure above to the
// pre-start `set_log_level` and not to the fixture.
#[test]
fn g0f_review_e1_control_without_prestart_level_delivers_errors() {
  let mut spec = FraktorSpec::new("G0fReviewE1Control");
  let bus = EventStream::new(spec.system().downgrade(), false);

  let destination = spec.test_actor();
  let logger = spec.system_mut().actor_of(Props::new(move || ForwardingLogger::new(destination.clone())));
  bus.start_default_loggers(&[logger], LogLevel::Info);

  bus.publish(Error::new(String::new(), 0, "boom"));

  spec.within(wait(), |spec| {
    spec.expect_msg_type::<Error>();
  });
  spec.terminate();
}
