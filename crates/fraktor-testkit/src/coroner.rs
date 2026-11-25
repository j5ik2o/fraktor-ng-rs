//! A watchdog that prints a report when a test runs longer than it should.
//!
//! Mirrors `org.apache.pekko.testkit.Coroner`. [`watch`] starts a thread that
//! waits for the given duration and, unless the returned [`WatchHandle`] is
//! cancelled first, prints a report to the given output. [`print_report`]
//! prints the same report at once.
//!
//! The report is what the standard library can tell about the process: the
//! title, the architecture and operating system, the available parallelism,
//! and the stack of the thread that prints it. Pekko's report also lists every
//! thread of the JVM and the deadlocks the JVM finds among monitors and
//! ownable synchronizers, and `watch` can print the JVM's thread counts. The
//! standard library can neither enumerate the threads of the process nor see
//! who holds a lock, so the port has none of these parts.

mod watch_handle;

use core::time::Duration;

use std::backtrace::Backtrace;
use std::io;
use std::io::Write;
use std::thread;

pub use watch_handle::WatchHandle;

#[cfg(test)]
#[path = "coroner_test.rs"]
mod tests;

/// How long `watch` waits for its thread to start, and `cancel` for it to
/// finish, as Pekko's `defaultStartAndStopDuration`.
pub const DEFAULT_START_AND_STOP_DURATION: Duration = Duration::from_secs(1);

/// The name of the thread that `watch` starts, as in Pekko.
const THREAD_NAME: &str = "Coroner";

/// Prints a report to `out` unless the returned handle is cancelled within
/// `duration`.
///
/// Mirrors `Coroner.watch` with the default start-and-stop duration and without
/// thread counts. The call returns once the watching thread has started, or
/// after [`DEFAULT_START_AND_STOP_DURATION`]. When `duration` passes first, the
/// thread writes `Coroner not cancelled after <ms>ms. Looking for signs of foul
/// play...` followed by the [`print_report`] report, and the handle's result
/// becomes `false`.
///
/// # Panics
///
/// Panics when the operating system refuses to start the thread.
pub fn watch<W>(duration: Duration, report_title: &str, out: W) -> WatchHandle
where
  W: Write + Send + 'static,
{
  let handle = WatchHandle::new(DEFAULT_START_AND_STOP_DURATION);
  let watched = handle.clone();
  let title = report_title.to_string();
  thread::Builder::new()
    .name(THREAD_NAME.to_string())
    .spawn(move || report_if_overdue(&watched, duration, &title, out))
    .expect("the operating system refused to start the Coroner thread");
  handle.wait_for_start();
  handle
}

/// Writes the report of the process to `out`.
///
/// Mirrors `Coroner.printReport`. The report starts with `Coroner's Report:
/// <title>` and ends with the stack of the calling thread.
///
/// # Errors
///
/// Returns the error of the first write to `out` that fails.
pub fn print_report(report_title: &str, out: &mut dyn Write) -> io::Result<()> {
  let processors = thread::available_parallelism().map_or_else(|_| "unknown".to_string(), |count| count.to_string());
  let current = thread::current();
  let thread_name = current.name().unwrap_or("<unnamed>");
  writeln!(out, "Coroner's Report: {report_title}")?;
  writeln!(out, "OS Architecture: {}", std::env::consts::ARCH)?;
  writeln!(out, "Operating system: {}", std::env::consts::OS)?;
  writeln!(out, "Available processors: {processors}")?;
  writeln!(out, "Stack of the reporting thread \"{thread_name}\":")?;
  writeln!(out, "{}", Backtrace::force_capture())?;
  Ok(())
}

/// The body of the watching thread.
fn report_if_overdue<W: Write>(handle: &WatchHandle, duration: Duration, title: &str, mut out: W) {
  handle.started();
  if handle.expire_unless_cancelled(duration) {
    let _ = report(duration, title, &mut out);
  }
  let _ = out.flush();
  handle.finished();
}

/// Writes the overdue notice and the report, and notes a failing report as
/// Pekko does.
fn report(duration: Duration, title: &str, out: &mut dyn Write) -> io::Result<()> {
  writeln!(out, "Coroner not cancelled after {}ms. Looking for signs of foul play...", duration.as_millis())?;
  if let Err(error) = print_report(title, out) {
    writeln!(out, "Error displaying Coroner's Report: {error}")?;
  }
  Ok(())
}
