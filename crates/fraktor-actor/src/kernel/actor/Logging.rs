//! The events an actor logs, mirroring `org.apache.pekko.event.Logging`.

pub type Mdc = alloc::collections::BTreeMap<alloc::string::String, alloc::string::String>;

mod debug;
mod error;
mod event_sink;
mod info;
mod initialize_logger;
mod log_event;
mod log_level;
mod log_marker;
mod log_marker_adapter;
mod logger_initialized;
mod warning;

pub use debug::Debug;
pub use error::Error;
pub use event_sink::EventSink;
pub use info::Info;
pub use initialize_logger::InitializeLogger;
pub use log_event::LogEvent;
pub use log_level::LogLevel;
pub use log_marker::LogMarker;
pub use log_marker_adapter::LogMarkerAdapter;
pub use logger_initialized::LoggerInitialized;
pub use warning::Warning;

/// Returns the empty diagnostic context that an event without one carries.
///
/// Mirrors `org.apache.pekko.event.Logging.emptyMDC`.
pub fn empty_mdc() -> Mdc {
  Mdc::new()
}
