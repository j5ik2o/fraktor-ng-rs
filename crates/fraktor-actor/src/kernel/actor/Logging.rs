//! The events an actor logs, mirroring `org.apache.pekko.event.Logging`.

pub type Mdc = alloc::collections::BTreeMap<alloc::string::String, alloc::string::String>;

mod error;
mod event_sink;
mod log_event;
mod log_level;
mod warning;

pub use error::Error;
pub use event_sink::EventSink;
pub use log_event::LogEvent;
pub use log_level::LogLevel;
pub use warning::Warning;

/// Returns the empty diagnostic context that an event without one carries.
///
/// Mirrors `org.apache.pekko.event.Logging.emptyMDC`.
pub fn empty_mdc() -> Mdc {
  Mdc::new()
}
