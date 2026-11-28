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

// The output of the console reporter, through its public entry point:
// `ConsoleReporter::new(verbose).report(&metrics, &mut Vec<u8>)`.
//
// The expected text is taken from `docs/design/issue-299/console-reporter-examples.expected`,
// the output of Pekko's `PekkoConsoleReporter` run on the original
// libraries, with three changes (design §15): the entries of a kind are in
// key order, the timer banner names `KnownOpsInTimespanTimer`, and lines
// end with the platform line separator. The `R` numbers are the items of
// design §15.
//
// These are not ports of Pekko cases and are not counted in the ledger.

use core::time::Duration;

use std::collections::BTreeMap;
use std::io;
use std::io::Write;
use std::sync::{Arc, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use fraktor_actor::kernel::pretty_duration::PrettyDurationInput;
use fraktor_testkit::metrics_kit::{
  AveragingGauge, ConsoleReporter, HdrHistogram, KnownOpsInTimespanTimer, Metric, MetricKey, MetricsError, MetricsKit,
  MetricsKitSettings,
};

/// The line separator of a report: `System.lineSeparator` of the original.
const NL: &str = if cfg!(windows) { "\r\n" } else { "\n" };

const AVERAGING_GAUGE_BANNER: &str = "-- AveragingGauge --------------------------------------------------------------";
const FILE_DESCRIPTOR_MAX_BANNER: &str =
  "-- FileDescriptorMetricSet$$anon$2 ---------------------------------------------";
const TIMER_BANNER: &str = "-- KnownOpsInTimespanTimer -----------------------------------------------------";
const HISTOGRAM_BANNER: &str = "-- HdrHistogram ----------------------------------------------------------------";

const HISTOGRAM_KEY: &str = "metrics-kit.hist.hdr-histogram";

fn ok<T>(result: Result<T, MetricsError>) -> T {
  result.unwrap_or_else(|error| panic!("unexpected metrics error: {error}"))
}

/// Each line followed by the report's line separator.
fn text(lines: &[&str]) -> String {
  lines.iter().map(|line| format!("{line}{NL}")).collect()
}

/// Each line followed by `\n`: the percentile distribution of a histogram
/// always ends its lines with `\n`, whatever the line separator.
fn distribution(lines: &[&str]) -> String {
  lines.iter().map(|line| format!("{line}\n")).collect()
}

/// Reports `metrics` into a buffer and returns the text and the result.
fn report(metrics: &BTreeMap<String, Metric>, verbose: bool) -> (String, io::Result<()>) {
  let mut output = Vec::new();
  let result = ConsoleReporter::new(verbose).report(metrics, &mut output);
  (String::from_utf8(output).expect("the report is UTF-8"), result)
}

fn histogram_of(values: &[i64]) -> HdrHistogram {
  let histogram = ok(HdrHistogram::new(100_000_000, 4, "ns"));
  for value in values {
    ok(histogram.update(*value));
  }
  histogram
}

fn only_histogram(histogram: HdrHistogram) -> BTreeMap<String, Metric> {
  BTreeMap::from([(HISTOGRAM_KEY.to_owned(), Metric::HdrHistogram(histogram))])
}

fn gauge_of(values: &[i64]) -> Metric {
  let gauge = AveragingGauge::new();
  gauge.add_slice(values);
  Metric::AveragingGauge(gauge)
}

/// The kit of a test that reports with its own reporter: no scheduled
/// report thread, and nothing printed to the standard output.
fn quiet_kit() -> MetricsKit {
  MetricsKit::with_output(MetricsKitSettings::default(), Box::new(io::sink()))
}

/// The nanoseconds since the Unix epoch, the clock the report of an
/// unstopped timer reads.
fn epoch_nanos() -> i128 {
  let since = SystemTime::now().duration_since(UNIX_EPOCH).expect("the clock is after the Unix epoch");
  i128::try_from(since.as_nanos()).expect("the clock fits in i128")
}

/// The lines of `output` split at the report's line separator.
fn lines_of(output: &str) -> Vec<&str> {
  output.split(NL).collect()
}

/// The text after `prefix`, or a failure naming the line.
fn value_after<'a>(line: &'a str, prefix: &str) -> &'a str {
  line.strip_prefix(prefix).unwrap_or_else(|| panic!("the line {line:?} does not start with {prefix:?}"))
}

fn is_digits(text: &str) -> bool {
  !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// `^[0-9]+\.[0-9]+(E-?[0-9]+)?$`: the shape of `Double.toString` of a
/// finite, non-negative value.
#[cfg(unix)]
fn is_java_double(text: &str) -> bool {
  let (mantissa, exponent) = match text.split_once('E') {
    | Some((mantissa, exponent)) => (mantissa, Some(exponent)),
    | None => (text, None),
  };
  let mantissa_ok = mantissa.split_once('.').is_some_and(|(whole, fraction)| is_digits(whole) && is_digits(fraction));
  let exponent_ok = match exponent {
    | Some(exponent) => is_digits(exponent.strip_prefix('-').unwrap_or(exponent)),
    | None => true,
  };
  mantissa_ok && exponent_ok
}

/// Parses `-d.ddde+NN ns` with exactly three fraction digits and the
/// exponent `exponent`, and returns the value it stands for.
fn negative_scientific_nanos(text: &str, exponent: u32) -> i128 {
  let suffix = format!("e+{exponent} ns");
  let mantissa = text
    .strip_prefix('-')
    .and_then(|rest| rest.strip_suffix(suffix.as_str()))
    .unwrap_or_else(|| panic!("{text:?} is not -d.ddde+{exponent} ns"));
  let (whole, fraction) = mantissa.split_once('.').unwrap_or_else(|| panic!("{text:?} has no fraction"));
  assert!(
    whole.len() == 1 && whole != "0" && is_digits(whole) && fraction.len() == 3 && is_digits(fraction),
    "{text:?} is not -d.ddde+{exponent} ns with a leading digit of 1 to 9"
  );
  let digits: i128 = format!("{whole}{fraction}").parse().expect("four digits");
  -digits * 10_i128.pow(exponent - 3)
}

// R1
#[test]
fn r1_an_empty_registry_prints_one_line_separator() {
  let (output, result) = report(&BTreeMap::new(), false);

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(output, NL);
}

// R6w
#[test]
fn r6w_the_measured_file_descriptors_report_this_process() {
  let kit = quiet_kit();
  ok(kit.measure_file_descriptors(&MetricKey::from_string("metrics-kit").child("file-desc")));

  let (output, result) = report(&kit.registered_metrics(), false);

  let max_key = "  metrics-kit.file-desc.file-descriptors.file-descriptors-max";
  #[cfg(windows)]
  {
    assert_eq!(output, text(&[FILE_DESCRIPTOR_MAX_BANNER, max_key]));
    assert_eq!(result.map_err(|error| error.kind()), Err(io::ErrorKind::Unsupported));
  }
  #[cfg(unix)]
  {
    let soft = soft_file_descriptor_limit();
    assert!(
      std::fs::read_dir("/proc/self/fd").or_else(|_| std::fs::read_dir("/dev/fd")).is_ok(),
      "precondition broken: neither /proc/self/fd nor /dev/fd can be read"
    );
    assert!(result.is_ok(), "{result:?}");
    let lines = lines_of(&output);
    assert_eq!(lines.len(), 10, "seven lines, two empty lines and the end: {output:?}");
    assert_eq!(lines[0], FILE_DESCRIPTOR_MAX_BANNER);
    assert_eq!(lines[1], max_key);
    assert_eq!(value_after(lines[2], "             value = "), soft.to_string());
    assert_eq!(lines[3], "  metrics-kit.file-desc.file-descriptors.file-descriptors-open");
    let open = value_after(lines[4], "             value = ");
    assert!(is_digits(open), "the open count {open:?} is not ^[0-9]+$");
    assert_eq!(lines[5], "  metrics-kit.file-desc.file-descriptors.file-descriptors-ratio");
    let ratio = value_after(lines[6], "             value = ");
    assert!(is_java_double(ratio), "the ratio {ratio:?} is not ^[0-9]+\\.[0-9]+(E-?[0-9]+)?$");
    assert_eq!(&lines[7..], ["", "", ""]);
  }
}

/// The soft limit of file descriptors of this process, read the way the
/// report reads it.
#[cfg(unix)]
#[allow(clippy::useless_conversion)]
fn soft_file_descriptor_limit() -> u64 {
  let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
  // SAFETY: `limit` is a writable `rlimit` owned by this call, and
  // `getrlimit` only writes to it.
  let result = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) };
  assert_eq!(result, 0, "precondition broken: getrlimit(RLIMIT_NOFILE) failed");
  u64::from(limit.rlim_cur)
}

// R8
#[test]
fn r8_averaging_gauges_print_double_to_string_and_half_up() {
  let metrics = BTreeMap::from([
    ("metrics-kit.a-empty".to_owned(), gauge_of(&[])),
    ("metrics-kit.b-tiny".to_owned(), gauge_of(&[[0_i64; 9_999].as_slice(), &[1]].concat())),
    ("metrics-kit.c-quarter".to_owned(), gauge_of(&[0, 0, 0, 25])),
    ("metrics-kit.d-whole".to_owned(), gauge_of(&[0, 4])),
    ("metrics-kit.e-eighth".to_owned(), gauge_of(&[0, 0, 0, 0, 0, 0, 0, 1])),
  ]);

  let (output, result) = report(&metrics, false);

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(&[
      AVERAGING_GAUGE_BANNER,
      "  metrics-kit.a-empty",
      "             value = NaN",
      "  metrics-kit.b-tiny",
      "             value = 1.0E-4",
      "  metrics-kit.c-quarter",
      "             value = 6.25",
      "  metrics-kit.d-whole",
      "             value = 2.0",
      "  metrics-kit.e-eighth",
      "             value = 0.125",
      "",
      AVERAGING_GAUGE_BANNER,
      "  metrics-kit.a-empty",
      "                avg = NaN",
      "  metrics-kit.b-tiny",
      "                avg = 0.00",
      "  metrics-kit.c-quarter",
      "                avg = 6.25",
      "  metrics-kit.d-whole",
      "                avg = 2.00",
      "  metrics-kit.e-eighth",
      "                avg = 0.13",
      "",
      "",
    ])
  );
}

// R9
#[test]
fn r9_keys_are_ordered_by_utf16_code_units() {
  let kit = quiet_kit();
  let private_use = MetricKey::from_string("metrics-kit").child("\u{E000}");
  let emoji = MetricKey::from_string("metrics-kit").child("\u{1F600}");
  ok(kit.average_gauge(&private_use)).add(1);
  ok(kit.average_gauge(&emoji)).add(2);

  let (output, result) = report(&kit.registered_metrics(), false);

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(&[
      AVERAGING_GAUGE_BANNER,
      "  metrics-kit.\u{1F600}",
      "             value = 2.0",
      "  metrics-kit.\u{E000}",
      "             value = 1.0",
      "",
      AVERAGING_GAUGE_BANNER,
      "  metrics-kit.\u{1F600}",
      "                avg = 2.00",
      "  metrics-kit.\u{E000}",
      "                avg = 1.00",
      "",
      "",
    ])
  );
}

/// An output that registers a metric in its kit on the first write, so a
/// metric is registered while a report is printed.
struct RegisteringOutput {
  kit: Weak<MetricsKit>,
  registered: bool,
  bytes: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl Write for RegisteringOutput {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    if !self.registered {
      self.registered = true;
      let kit = self.kit.upgrade().expect("the kit is alive while it reports");
      ok(kit.average_gauge(&MetricKey::from_string("metrics-kit").child("late")));
    }
    self.bytes.lock().expect("bytes lock").extend_from_slice(buf);
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

// R12
#[test]
fn r12_a_metric_registered_while_reporting_stays_registered() {
  let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
  let kit = Arc::new_cyclic(|weak: &Weak<MetricsKit>| {
    MetricsKit::with_output(
      MetricsKitSettings::default(),
      Box::new(RegisteringOutput { kit: weak.clone(), registered: false, bytes: Arc::clone(&bytes) }),
    )
  });
  ok(kit.average_gauge(&MetricKey::from_string("metrics-kit").child("avg-size")));

  kit.report_and_clear_metrics();

  let keys: Vec<String> = kit.registered_metrics().into_keys().collect();
  assert_eq!(keys, ["metrics-kit.late"]);
}

// R13
#[test]
fn r13_an_unstopped_timer_reports_the_negated_start_time() {
  let before = epoch_nanos();
  let timer = KnownOpsInTimespanTimer::new(10);
  let after = epoch_nanos();
  assert!(
    10_i128.pow(18) <= before && before <= after && after <= i128::from(i64::MAX),
    "precondition broken: the clock is outside 2001-09-09 to 2262-04-11 ({before}, {after})"
  );
  let metrics = BTreeMap::from([("metrics-kit".to_owned(), Metric::KnownOpsInTimespanTimer(timer))]);

  let (output, result) = report(&metrics, false);

  assert!(result.is_ok(), "{result:?}");
  let lines = lines_of(&output);
  assert_eq!(lines.len(), 9, "six lines, two empty lines and the end: {output:?}");
  assert_eq!(lines[0], TIMER_BANNER);
  assert_eq!(lines[1], "  metrics-kit");
  assert_eq!(lines[2], "               ops = 10");
  let time = negative_scientific_nanos(value_after(lines[3], "              time = "), 18);
  assert!(
    -after - 500_000_000_000_000 <= time && time <= -before + 500_000_000_000_000,
    "time {time} is not the negated start time between {before} and {after}"
  );
  assert_eq!(lines[4], "             ops/s = -0.00");
  let avg = negative_scientific_nanos(value_after(lines[5], "               avg = "), 17);
  assert!(
    -after / 10 - 50_000_000_000_000 <= avg && avg <= -before / 10 + 50_000_000_000_000,
    "avg {avg} is not a tenth of the negated start time between {before} and {after}"
  );
  assert_eq!(&lines[6..], ["", "", ""]);
}

// R14
#[test]
fn r14_a_stopped_timer_reports_its_public_readings() {
  let timer = KnownOpsInTimespanTimer::new(10);
  assert!(timer.stop());
  let metrics = BTreeMap::from([("metrics-kit".to_owned(), Metric::KnownOpsInTimespanTimer(timer.clone()))]);

  let (output, result) = report(&metrics, false);

  assert!(result.is_ok(), "{result:?}");
  let elapsed = timer.elapsed().expect("the timer is stopped");
  let avg_nanos = timer.avg_duration_nanos().expect("the timer is stopped");
  let ops_per_second = timer.ops_per_second().expect("the timer is stopped");
  let lines = lines_of(&output);
  assert_eq!(lines.len(), 9, "six lines, two empty lines and the end: {output:?}");
  assert_eq!(lines[0], TIMER_BANNER);
  assert_eq!(lines[1], "  metrics-kit");
  assert_eq!(lines[2], "               ops = 10");
  assert_eq!(value_after(lines[3], "              time = "), PrettyDurationInput::Finite(elapsed).pretty());
  let ops = value_after(lines[4], "             ops/s = ");
  if ops == "Infinity" {
    assert!(ops_per_second.is_infinite() && ops_per_second > 0.0, "ops/s {ops:?} for {ops_per_second}");
  } else {
    let (whole, fraction) = ops.split_once('.').unwrap_or_else(|| panic!("ops/s {ops:?} has no fraction"));
    assert!(
      is_digits(whole) && fraction.len() == 2 && is_digits(fraction),
      "ops/s {ops:?} is not ^[0-9]+\\.[0-9]{{2}}$"
    );
    let printed: f64 = ops.parse().expect("a decimal");
    assert!((printed - ops_per_second).abs() <= 0.005, "ops/s {ops:?} for {ops_per_second}");
  }
  let avg = Duration::from_nanos(u64::try_from(avg_nanos).expect("a stopped timer has a non-negative average"));
  assert_eq!(value_after(lines[5], "               avg = "), PrettyDurationInput::Finite(avg).pretty());
  assert_eq!(&lines[6..], ["", "", ""]);
}

/// The nine lines of the histogram of `0..=4`.
const ZERO_TO_FOUR_LINES: [&str; 11] = [
  HISTOGRAM_BANNER,
  "  metrics-kit.hist.hdr-histogram",
  "               min = 0 ns",
  "               max = 4 ns",
  "              mean = 2.00 ns",
  "            stddev = 1.41",
  "              75% <= 3 ns",
  "              95% <= 4 ns",
  "              98% <= 4 ns",
  "              99% <= 4 ns",
  "            99.9% <= 4 ns",
];

// R15
#[test]
fn r15_a_verbose_histogram_prints_its_percentile_distribution() {
  let (output, result) = report(&only_histogram(histogram_of(&[0, 1, 2, 3, 4])), true);

  assert!(result.is_ok(), "{result:?}");
  let expected = text(&ZERO_TO_FOUR_LINES)
    + &distribution(&[
      "       Value     Percentile TotalCount 1/(1-Percentile)",
      "",
      "      0.0000 0.000000000000          1           1.00",
      "      0.0000 0.100000000000          1           1.11",
      "      0.0000 0.200000000000          1           1.25",
      "      1.0000 0.300000000000          2           1.43",
      "      1.0000 0.400000000000          2           1.67",
      "      2.0000 0.500000000000          3           2.00",
      "      2.0000 0.550000000000          3           2.22",
      "      2.0000 0.600000000000          3           2.50",
      "      3.0000 0.650000000000          4           2.86",
      "      3.0000 0.700000000000          4           3.33",
      "      3.0000 0.750000000000          4           4.00",
      "      3.0000 0.775000000000          4           4.44",
      "      3.0000 0.800000000000          4           5.00",
      "      4.0000 0.825000000000          5           5.71",
      "      4.0000 1.000000000000          5",
      "#[Mean    =       2.0000, StdDeviation   =       1.4142]",
      "#[Max     =       4.0000, Total count    =            5]",
      "#[Buckets =           13, SubBuckets     =        32768]",
    ])
    + &text(&["", ""]);
  assert_eq!(output, expected);
}

// R16
#[test]
fn r16_a_quiet_histogram_prints_no_distribution() {
  let (output, result) = report(&only_histogram(histogram_of(&[0, 1, 2, 3, 4])), false);

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(output, text(&ZERO_TO_FOUR_LINES) + &text(&["", ""]));
}

// R17
#[test]
fn r17_an_empty_verbose_histogram_prints_zeros_and_an_empty_distribution() {
  let (output, result) = report(&only_histogram(histogram_of(&[])), true);

  assert!(result.is_ok(), "{result:?}");
  let expected = text(&[
    HISTOGRAM_BANNER,
    "  metrics-kit.hist.hdr-histogram",
    "               min = 0 ns",
    "               max = 0 ns",
    "              mean = 0.00 ns",
    "            stddev = 0.00",
    "              75% <= 0 ns",
    "              95% <= 0 ns",
    "              98% <= 0 ns",
    "              99% <= 0 ns",
    "            99.9% <= 0 ns",
  ]) + &distribution(&[
    "       Value     Percentile TotalCount 1/(1-Percentile)",
    "",
    "#[Mean    =       0.0000, StdDeviation   =       0.0000]",
    "#[Max     =       0.0000, Total count    =            0]",
    "#[Buckets =           13, SubBuckets     =        32768]",
  ]) + &text(&["", ""]);
  assert_eq!(output, expected);
}

// R18
#[test]
fn r18_a_histogram_of_seven_ones_and_a_two() {
  let (output, result) = report(&only_histogram(histogram_of(&[1, 1, 1, 1, 1, 1, 1, 2])), false);

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(&[
      HISTOGRAM_BANNER,
      "  metrics-kit.hist.hdr-histogram",
      "               min = 1 ns",
      "               max = 2 ns",
      "              mean = 1.13 ns",
      "            stddev = 0.33",
      "              75% <= 1 ns",
      "              95% <= 2 ns",
      "              98% <= 2 ns",
      "              99% <= 2 ns",
      "            99.9% <= 2 ns",
      "",
      "",
    ])
  );
}

// R19
#[test]
fn r19_the_histogram_of_case_4_input() {
  let histogram = ok(HdrHistogram::new(100_000_000, 4, "ns"));
  for _ in 1..=11 {
    for value in 0_i64..=1_579_331 {
      ok(histogram.update(value));
    }
  }
  ok(histogram.update(1_579_331));

  let (output, result) = report(&only_histogram(histogram), false);

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(&[
      HISTOGRAM_BANNER,
      "  metrics-kit.hist.hdr-histogram",
      "               min = 0 ns",
      "               max = 1579391 ns",
      "              mean = 789666.04 ns",
      "            stddev = 455913.92",
      "              75% <= 1184511 ns",
      "              95% <= 1500415 ns",
      "              98% <= 1547775 ns",
      "              99% <= 1563583 ns",
      "            99.9% <= 1577791 ns",
      "",
      "",
    ])
  );
}

// R22
#[test]
fn r22_only_a_histogram_prints_only_the_histogram_banner() {
  let (output, result) = report(&only_histogram(histogram_of(&[5])), false);

  assert!(result.is_ok(), "{result:?}");
  let banners: Vec<&str> = lines_of(&output).into_iter().filter(|line| line.starts_with("-- ")).collect();
  assert_eq!(banners, [HISTOGRAM_BANNER]);
  for other in ["-- AveragingGauge", "-- KnownOpsInTimespanTimer", "-- FileDescriptor"] {
    assert!(!lines_of(&output).iter().any(|line| line.starts_with(other)), "{other:?} printed: {output:?}");
  }
  assert!(output.ends_with(&format!("{NL}{NL}")), "{output:?}");
}

/// An output whose writes always fail.
struct FailingOutput;

impl Write for FailingOutput {
  fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
    Err(io::Error::new(io::ErrorKind::BrokenPipe, "the output is broken"))
  }

  fn flush(&mut self) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::BrokenPipe, "the output is broken"))
  }
}

// R23
#[test]
fn r23_a_failing_output_fails_the_report() {
  let metrics = BTreeMap::from([("metrics-kit.avg-size".to_owned(), gauge_of(&[2, 3]))]);

  let result = ConsoleReporter::new(false).report(&metrics, &mut FailingOutput);

  assert!(result.is_err());
}
