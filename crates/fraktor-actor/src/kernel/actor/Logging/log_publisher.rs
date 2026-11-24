//! The logger that publishes a formatted log event.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use core::fmt;

use crate::kernel::actor::Logging::{empty_mdc, Debug, Error, Info, LogClass, LogEvent, LogLevel, Mdc, Warning};
use crate::kernel::event::EventStream;
use crate::kernel::event::EventStreamBusWeak;

/// Publishes a formatted log event on an event stream.
///
/// Mirrors `org.apache.pekko.event.LoggingAdapter` together with
/// `DiagnosticLoggingAdapter`: it is built from an event stream, a log source
/// and a log class, formats a template with its arguments at one level, and
/// holds a diagnostic context that a later call may replace or clear.
///
/// Pekko reaches the adapter through `Logging.getLogger(system, this)` and mixes
/// the arity overloads `error(message)` and `error(template, arg)`. Rust has no
/// same-name arity overloads, so this adapter has one entry for every level and
/// argument count: [`LogPublisher::log`] takes the level, the template and one
/// slice of arguments. An empty slice is the plain message. The cause-carrying
/// entry [`LogPublisher::error`] is separate and only publishes at
/// [`LogLevel::Error`], so no other level ever carries a cause.
///
/// The arguments are one generic slice of `fmt::Display`, so a caller passes a
/// primitive array such as `[10_i32, 20, 30]` by reference without erasing the
/// element type, and a heterogeneous array of `&dyn fmt::Display` values
/// directly.
///
/// Direct subscribers receive the events they subscribe to at every level.
/// [`EventStream::set_log_level`] controls the subscriptions of default loggers.
///
/// The publisher holds a weak reference to the shared subscription table,
/// allowing a `Props` factory to capture it. Publishing stops when the last
/// strong reference to the table is dropped. An unsubscriber can keep the
/// table alive after the last [`EventStream`] handle is dropped.
#[derive(Clone)]
pub struct LogPublisher {
  bus: EventStreamBusWeak,
  log_source: String,
  log_class: LogClass,
  mdc: Mdc,
}

impl LogPublisher {
  /// Creates a publisher that publishes on `event_stream`.
  ///
  /// Mirrors `Logging.getLogger(system, source)`: the source is reported as the
  /// source of every event, and the log class is carried unchanged. The
  /// diagnostic context starts empty, as Pekko's adapter starts without one.
  ///
  /// Borrows the stream to obtain a weak reference to its subscription table.
  /// Calls to [`Self::log`] and [`Self::error`] deliver events while the stream
  /// or another strong owner keeps that table alive.
  pub fn new(event_stream: &EventStream, log_source: &str, log_class: LogClass) -> Self {
    Self { bus: event_stream.downgrade(), log_source: log_source.to_string(), log_class, mdc: empty_mdc() }
  }

  /// Replaces the diagnostic context that later events carry.
  ///
  /// Mirrors `DiagnosticLoggingAdapter.setMDC`. Pekko's adapter takes the map
  /// or `null`, and its `clearMDC` sets an empty map rather than a null one, so
  /// both inputs the caller can give here normalize to the same empty context:
  /// `Some` installs the given map, an explicit empty map installs an empty
  /// context, and `None` records that no context was given, which is also
  /// published as the empty context. The adapter owns a copy, so a later change
  /// does not reach an event that was already published.
  pub fn set_mdc(&mut self, mdc: Option<&Mdc>) {
    self.mdc = mdc.cloned().unwrap_or_default();
  }

  /// Clears the diagnostic context so the next event carries none.
  ///
  /// Mirrors `DiagnosticLoggingAdapter.clearMDC`, whose implementation resets
  /// the context to an empty map: later events carry the empty context, the same
  /// context an absent one publishes.
  pub fn clear_mdc(&mut self) {
    self.mdc = empty_mdc();
  }

  /// Publishes an event at `level` whose message formats `template` with `args`.
  ///
  /// The message is filled left to right, one argument per `{}` placeholder,
  /// and the argument text is never scanned again, mirroring Pekko's
  /// `formatImpl`. Fewer arguments than placeholders leave the unmatched `{}`
  /// as written. More arguments append ` WARNING arguments left: <n>` after the
  /// formatted text, where `<n>` is the number of arguments that were not
  /// consumed.
  pub fn log<A: fmt::Display>(&self, level: LogLevel, template: &str, args: &[A]) {
    let message = format_template(template, args);
    let mdc = self.mdc.clone();
    let log_class = self.log_class.value();
    match level {
      | LogLevel::Error => self.publish(Error::new(self.log_source.clone(), log_class, &message).with_mdc(mdc)),
      | LogLevel::Warning => {
        self.publish(Warning::new(self.log_source.clone(), log_class, &message).with_mdc(mdc));
      },
      | LogLevel::Info => self.publish(Info::new(self.log_source.clone(), log_class, &message).with_mdc(mdc)),
      | LogLevel::Debug => self.publish(Debug::new(self.log_source.clone(), log_class, &message).with_mdc(mdc)),
    }
  }

  /// Publishes an error event that carries `cause` itself and formats
  /// `template` with `args`.
  ///
  /// Mirrors `DiagnosticLoggingAdapter.error(cause, template, args)`. The
  /// delivered error keeps the same shared cause, so a subscriber recovers it
  /// through [`Error::cause`] and compares it with the sent one by identity,
  /// which is the original's `assertSame`. The cause's `Debug` text still stands
  /// in for the cause name a filter compares, and its `Display` text for the
  /// cause message, as the existing constructors do.
  pub fn error<C, A>(&self, cause: Arc<C>, template: &str, args: &[A])
  where
    C: fmt::Debug + fmt::Display + Send + Sync + 'static,
    A: fmt::Display,
  {
    let message = format_template(template, args);
    let mdc = self.mdc.clone();
    let event =
      Error::with_shared_cause(cause, self.log_source.clone(), self.log_class.value(), &message).with_mdc(mdc);
    self.publish(event);
  }

  fn publish<E: LogEvent + Send + Sync + 'static>(&self, event: E) {
    self.bus.publish(event);
  }
}

/// Formats `template` with `args`, mirroring `formatImpl` in Pekko's
/// `Logging.scala`.
///
/// Every `{}` is replaced left to right with the display text of the next
/// argument; the replacement text is appended as written and never scanned for
/// further placeholders. A missing argument leaves its `{}` unchanged. Any
/// argument beyond the placeholders is reported as
/// ` WARNING arguments left: <n>`, where `<n>` is how many were not consumed.
pub(crate) fn format_template<A: fmt::Display>(template: &str, args: &[A]) -> String {
  let mut rendered = String::new();
  let mut next = 0;
  let mut rest = template;
  while let Some(open) = rest.find("{}") {
    rendered.push_str(&rest[..open]);
    match args.get(next) {
      | Some(argument) => {
        rendered.push_str(&format!("{}", argument));
        next += 1;
      },
      | None => rendered.push_str("{}"),
    }
    rest = &rest[open + 2..];
  }
  rendered.push_str(rest);
  let left = args.len().saturating_sub(next);
  if left > 0 {
    rendered.push_str(&format!(" WARNING arguments left: {}", left));
  }
  rendered
}
