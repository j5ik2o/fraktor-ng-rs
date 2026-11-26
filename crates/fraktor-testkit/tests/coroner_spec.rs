// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   testkit/src/test/scala/org/apache/pekko/testkit/CoronerSpec.scala
//
// Cases (original line numbers):
//   - "generate a report if enough time passes" (L37-45): ported.
//   - "not generate a report if cancelled early" (L47-54): ported.
//   - "display thread counts if enabled" (L56-66): excluded with the owner's
//     approval (ledger cases.excluded). It reads the JVM's ThreadMXBean.
//   - "display deadlock information in its report" (L68-152): excluded with
//     the owner's approval (ledger cases.excluded). It relies on the JVM
//     tracking the owner of each ReentrantLock.
//   The tests that stand in for these two are the unit tests of
//   crates/fraktor-testkit/src/coroner_test.rs.
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
