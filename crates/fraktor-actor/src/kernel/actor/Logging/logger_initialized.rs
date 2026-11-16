//! The answer a logger gives once it has subscribed.

/// The answer a logger gives to [`InitializeLogger`](super::InitializeLogger).
///
/// Mirrors `org.apache.pekko.event.Logging.LoggerInitialized`. The port keeps
/// only the message: Pekko also has an abstract parent class for Java
/// implementations, which no ported test uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoggerInitialized;
