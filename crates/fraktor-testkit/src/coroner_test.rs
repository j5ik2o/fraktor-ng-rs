//! Unit tests for the Coroner.
//!
//! The ported `CoronerSpec` keeps two of its four cases; the owner approved
//! excluding the thread-count and deadlock cases, which rest on the JVM's
//! thread management, and asked for tests of what the port can report instead.
//! These are those tests: the shape of the report, the stack it carries, and
//! the outcome of a watch.

use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use super::print_report;
use super::watch;

#[derive(Clone, Default)]
struct Captured {
  bytes: Arc<Mutex<Vec<u8>>>,
}

impl Captured {
  fn text(&self) -> String {
    String::from_utf8(self.bytes.lock().unwrap().clone()).unwrap()
  }
}

impl Write for Captured {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.bytes.lock().unwrap().extend_from_slice(buf);
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

struct Refusing;

impl Write for Refusing {
  fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
    Err(io::Error::other("refused"))
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

fn report_of(title: &str) -> String {
  let mut out = Captured::default();
  print_report(title, &mut out).unwrap();
  out.text()
}

#[test]
fn print_report_starts_with_the_title_and_lists_the_process_facts_in_order() {
  let report = report_of("Shape test");
  let lines: Vec<&str> = report.lines().collect();

  assert_eq!(lines[0], "Coroner's Report: Shape test");
  assert_eq!(lines[1], format!("OS Architecture: {}", std::env::consts::ARCH));
  assert_eq!(lines[2], format!("Operating system: {}", std::env::consts::OS));
  let processors = lines[3].strip_prefix("Available processors: ").expect("the processors line");
  assert!(processors.parse::<usize>().is_ok_and(|count| count > 0) || processors == "unknown");
}

#[test]
fn print_report_carries_the_stack_of_the_reporting_thread() {
  let report = report_of("Stack test");
  let name = std::thread::current().name().unwrap().to_string();

  let heading = format!("Stack of the reporting thread \"{name}\":");
  let (_, stack) = report.split_once(&heading).expect("the stack heading names the calling thread");
  assert!(!stack.trim().is_empty(), "report: {report}");
}

#[test]
fn print_report_returns_the_error_of_a_failing_output() {
  let error = print_report("Failing output", &mut Refusing).expect_err("the output refused every write");

  assert_eq!(error.to_string(), "refused");
}

#[test]
fn an_expired_watch_announces_the_overrun_before_the_report_of_the_coroner_thread() {
  let out = Captured::default();
  let coroner = watch(Duration::from_millis(50), "Overrun", out.clone());

  assert!(!coroner.result(Duration::from_secs(5)));
  coroner.cancel();

  let report = out.text();
  assert!(
    report.starts_with("Coroner not cancelled after 50ms. Looking for signs of foul play...\n"),
    "report: {report}"
  );
  assert!(report.contains("Coroner's Report: Overrun"));
  assert!(report.contains("Stack of the reporting thread \"Coroner\":"));
}

#[test]
fn a_cancelled_watch_reports_true_and_writes_nothing() {
  let out = Captured::default();
  let coroner = watch(Duration::from_secs(60), "Cancelled", out.clone());

  coroner.cancel();

  assert!(coroner.result(Duration::from_secs(1)));
  assert_eq!(out.text(), "");
}

#[test]
fn cancelling_an_expired_watch_keeps_it_expired() {
  let out = Captured::default();
  let coroner = watch(Duration::from_millis(10), "Expired", out);
  coroner.ready(Duration::from_secs(5));

  coroner.cancel();

  assert!(!coroner.result(Duration::from_secs(1)));
}

#[test]
fn result_of_a_running_watch_is_false_when_max_passes_first() {
  let coroner = watch(Duration::from_secs(60), "Running", Captured::default());

  assert!(!coroner.result(Duration::from_millis(20)));
  coroner.cancel();
}
