// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   testkit/src/test/scala/org/apache/pekko/testkit/CoronerSpec.scala
//
// Cases (original line numbers):
//   - "generate a report if enough time passes" (L37-45): ported.
//   - "not generate a report if cancelled early" (L47-54): ported.
//   - "display thread counts if enabled" (L56-66): not ported yet. It reads the
//     JVM's ThreadMXBean thread count; how to treat it is undecided.
//   - "display deadlock information in its report" (L68-152): not ported yet.
//     It relies on the JVM tracking the owner of each ReentrantLock; how to
//     treat it is undecided.
//
// Original test names are kept as comments next to each ported case.

use core::time::Duration;

use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

use fraktor_testkit::coroner;

/// The `ByteArrayOutputStream` that the original's `captureOutput` writes to.
#[derive(Clone, Default)]
struct CapturedOutput {
  bytes: Arc<Mutex<Vec<u8>>>,
}

impl CapturedOutput {
  fn text(&self) -> String {
    String::from_utf8(self.bytes.lock().unwrap().clone()).expect("the report is UTF-8")
  }
}

impl Write for CapturedOutput {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.bytes.lock().unwrap().extend_from_slice(buf);
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

// The original's `captureOutput`: runs `f` with a fresh output and returns
// what it wrote.
fn capture_output<A>(f: impl FnOnce(CapturedOutput) -> A) -> (A, String) {
  let out = CapturedOutput::default();
  let result = f(out.clone());
  (result, out.text())
}

// "generate a report if enough time passes"
#[test]
fn generate_a_report_if_enough_time_passes() {
  let (_, report) = capture_output(|out| {
    let coroner = coroner::watch(Duration::from_millis(100), "XXXX", out);
    coroner.ready(Duration::from_secs(5));
    coroner.cancel();
  });
  assert!(report.contains("Coroner's Report"), "report: {report}");
  assert!(report.contains("XXXX"), "report: {report}");
}

// "not generate a report if cancelled early"
#[test]
fn not_generate_a_report_if_cancelled_early() {
  let (_, report) = capture_output(|out| {
    let coroner = coroner::watch(Duration::from_secs(60), "XXXX", out);
    coroner.cancel();
    coroner.ready(Duration::from_secs(1));
  });
  assert_eq!(report, "");
}
