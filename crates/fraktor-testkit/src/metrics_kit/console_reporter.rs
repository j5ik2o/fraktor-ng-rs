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

//! The reporter that prints the registered metrics.

use core::time::Duration;
use std::collections::BTreeMap;
use std::io;
use std::io::Write;

use fraktor_actor::kernel::pretty_duration::PrettyDurationInput;

use crate::metrics_kit::Metric;

#[cfg(test)]
#[path = "console_reporter_test.rs"]
mod tests;

/// One metric, read for printing: the values a report prints, taken under
/// the locks and from the operating system and the clock before any line
/// is written.
pub(crate) enum MetricReading {
  /// An averaging gauge, with its current value.
  AveragingGauge { value: f64 },
  /// A file descriptor gauge, with its current reading.
  FileDescriptor(FileDescriptorReading),
  /// A timer, with its operation count and its signed elapsed nanoseconds.
  /// An unstopped timer reads as the negation of its start time.
  KnownOpsInTimespanTimer { ops: i64, elapsed_nanos: i64 },
  /// A histogram, with a snapshot of its recorded values and its unit.
  HdrHistogram { snapshot: hdrhistogram::Histogram<u64>, unit: String },
}

/// The reading of one file descriptor gauge.
pub(crate) enum FileDescriptorReading {
  /// The number of open file descriptors, or `None` where the platform
  /// has no reading.
  Open(Option<u64>),
  /// The file descriptor limit, or `None` where the platform has no
  /// reading.
  Max(Option<u64>),
  /// The ratio of open descriptors to the limit.
  Ratio(f64),
}

impl FileDescriptorReading {
  /// The name the banner of a section headed by this reading uses.
  fn banner_name(&self) -> &'static str {
    match self {
      | Self::Open(_) => "FileDescriptorMetricSet$$anon$1",
      | Self::Max(_) => "FileDescriptorMetricSet$$anon$2",
      | Self::Ratio(_) => "FileDescriptorRatioGauge",
    }
  }
}

/// Prints every registered metric to an output.
///
/// Mirrors `org.apache.pekko.testkit.metrics.reporter.PekkoConsoleReporter`.
pub struct ConsoleReporter {
  verbose: bool,
}

impl ConsoleReporter {
  /// Creates a reporter. `verbose` adds the percentile distribution of
  /// histograms to the report.
  pub fn new(verbose: bool) -> Self {
    Self { verbose }
  }

  /// Prints `metrics`, grouped by kind and ordered by key within a kind.
  ///
  /// # Errors
  ///
  /// Returns the first write or flush error of `output`. Lines written
  /// before the error stay written, so reporting the same metrics again
  /// after a failure can print those lines twice. A file descriptor gauge
  /// without a reading on this platform fails the report with
  /// [`io::ErrorKind::Unsupported`] after its key line, and a timer whose
  /// average is the smallest `i64` fails it with
  /// [`io::ErrorKind::InvalidInput`] after its operations per second line.
  pub fn report(&self, metrics: &BTreeMap<String, Metric>, output: &mut dyn Write) -> io::Result<()> {
    let readings: Vec<(String, MetricReading)> = metrics
      .iter()
      .map(|(key, metric)| {
        let reading = match metric {
          | Metric::AveragingGauge(gauge) => MetricReading::AveragingGauge { value: gauge.value() },
          | Metric::FileDescriptorGauge(gauge) => MetricReading::FileDescriptor(gauge.reading()),
          | Metric::HdrHistogram(histogram) => histogram
            .with_snapshot(|snapshot| MetricReading::HdrHistogram { snapshot, unit: histogram.unit().to_owned() }),
          | Metric::KnownOpsInTimespanTimer(timer) => MetricReading::KnownOpsInTimespanTimer {
            ops: timer.count(),
            elapsed_nanos: timer.elapsed_nanos_for_report(),
          },
        };
        (key.clone(), reading)
      })
      .collect();
    let line_separator = if cfg!(windows) { "\r\n" } else { "\n" };
    render(&readings, self.verbose, line_separator, output)
  }
}

/// Writes one line: the text, then `line_separator`.
fn write_line(output: &mut dyn Write, text: &str, line_separator: &str) -> io::Result<()> {
  output.write_all(text.as_bytes())?;
  output.write_all(line_separator.as_bytes())
}

/// The banner line of a section: `-- `, the name, a space, and enough
/// dashes to make the line 80 characters long.
fn banner(name: &str) -> String {
  format!("-- {name} {}", "-".repeat(76_usize.saturating_sub(name.len())))
}

/// The readings of one section, in UTF-16 code unit order of their keys.
fn sorted(
  readings: &[(String, MetricReading)],
  keep: impl Fn(&MetricReading) -> bool,
) -> Vec<&(String, MetricReading)> {
  let mut section: Vec<&(String, MetricReading)> = readings.iter().filter(|(_, reading)| keep(reading)).collect();
  section.sort_by_cached_key(|(key, _)| key.encode_utf16().collect::<Vec<u16>>());
  section
}

/// Writes the report of `readings` to `output`.
///
/// The readings are printed as gauges, then timers, then histograms, then
/// averaging gauges again, each section headed by a banner naming its
/// first entry and followed by an empty line, and one more empty line
/// ends the report.
pub(crate) fn render(
  readings: &[(String, MetricReading)],
  verbose: bool,
  line_separator: &str,
  output: &mut dyn Write,
) -> io::Result<()> {
  render_gauges(readings, line_separator, output)?;
  render_timers(readings, line_separator, output)?;
  render_histograms(readings, verbose, line_separator, output)?;
  render_averaging_gauges(readings, line_separator, output)?;
  write_line(output, "", line_separator)?;
  output.flush()
}

/// The gauge section: averaging gauges and file descriptor gauges, each
/// printed with its value.
fn render_gauges(readings: &[(String, MetricReading)], line_separator: &str, output: &mut dyn Write) -> io::Result<()> {
  let section = sorted(readings, |reading| {
    matches!(reading, MetricReading::AveragingGauge { .. } | MetricReading::FileDescriptor(_))
  });
  if section.is_empty() {
    return Ok(());
  }
  let first = match &section[0].1 {
    | MetricReading::AveragingGauge { .. } => "AveragingGauge",
    | MetricReading::FileDescriptor(reading) => reading.banner_name(),
    | _ => unreachable!("the gauge section holds only gauges"),
  };
  write_line(output, &banner(first), line_separator)?;
  for (key, reading) in &section {
    write_line(output, &format!("  {key}"), line_separator)?;
    match reading {
      | MetricReading::AveragingGauge { value } => {
        write_line(output, &format!("             value = {}", java_double_to_string(*value)), line_separator)?;
      },
      | MetricReading::FileDescriptor(FileDescriptorReading::Open(Some(count))) => {
        write_line(output, &format!("             value = {count}"), line_separator)?;
      },
      | MetricReading::FileDescriptor(FileDescriptorReading::Max(Some(limit))) => {
        write_line(output, &format!("             value = {limit}"), line_separator)?;
      },
      | MetricReading::FileDescriptor(FileDescriptorReading::Ratio(ratio)) => {
        write_line(output, &format!("             value = {}", java_double_to_string(*ratio)), line_separator)?;
      },
      | MetricReading::FileDescriptor(FileDescriptorReading::Open(None) | FileDescriptorReading::Max(None)) => {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "no file descriptor reading on this platform"));
      },
      | _ => unreachable!("the gauge section holds only gauges"),
    }
  }
  write_line(output, "", line_separator)
}

/// The averaging gauge section: each averaging gauge printed with its
/// average.
fn render_averaging_gauges(
  readings: &[(String, MetricReading)],
  line_separator: &str,
  output: &mut dyn Write,
) -> io::Result<()> {
  let section = sorted(readings, |reading| matches!(reading, MetricReading::AveragingGauge { .. }));
  if section.is_empty() {
    return Ok(());
  }
  write_line(output, &banner("AveragingGauge"), line_separator)?;
  for (key, reading) in &section {
    let MetricReading::AveragingGauge { value } = reading else {
      unreachable!("the averaging gauge section holds only averaging gauges")
    };
    write_line(output, &format!("  {key}"), line_separator)?;
    write_line(output, &format!("                avg = {}", format_fixed(*value, 2, 2)), line_separator)?;
  }
  write_line(output, "", line_separator)
}

/// The timer section.
fn render_timers(readings: &[(String, MetricReading)], line_separator: &str, output: &mut dyn Write) -> io::Result<()> {
  let section = sorted(readings, |reading| matches!(reading, MetricReading::KnownOpsInTimespanTimer { .. }));
  if section.is_empty() {
    return Ok(());
  }
  write_line(output, &banner("KnownOpsInTimespanTimer"), line_separator)?;
  for (key, reading) in &section {
    let MetricReading::KnownOpsInTimespanTimer { ops, elapsed_nanos } = reading else {
      unreachable!("the timer section holds only timers")
    };
    write_line(output, &format!("  {key}"), line_separator)?;
    write_line(output, &format!("               ops = {ops}"), line_separator)?;
    write_line(output, &format!("              time = {}", pretty_signed_nanos(*elapsed_nanos)), line_separator)?;
    let ops_per_second = *ops as f64 / (*elapsed_nanos as f64 / 1_000_000_000.0);
    write_line(output, &format!("             ops/s = {}", format_fixed(ops_per_second, 2, 2)), line_separator)?;
    // The conversion saturates as the original's conversion to a long
    // does, and a NaN average becomes zero.
    let avg_nanos = (*elapsed_nanos as f64 / *ops as f64) as i64;
    if avg_nanos == i64::MIN {
      return Err(io::Error::new(io::ErrorKind::InvalidInput, "the average duration is out of range"));
    }
    write_line(output, &format!("               avg = {}", pretty_signed_nanos(avg_nanos)), line_separator)?;
  }
  write_line(output, "", line_separator)
}

/// The histogram section.
fn render_histograms(
  readings: &[(String, MetricReading)],
  verbose: bool,
  line_separator: &str,
  output: &mut dyn Write,
) -> io::Result<()> {
  let section = sorted(readings, |reading| matches!(reading, MetricReading::HdrHistogram { .. }));
  if section.is_empty() {
    return Ok(());
  }
  write_line(output, &banner("HdrHistogram"), line_separator)?;
  for (key, reading) in &section {
    let MetricReading::HdrHistogram { snapshot, unit } = reading else {
      unreachable!("the histogram section holds only histograms")
    };
    write_line(output, &format!("  {key}"), line_separator)?;
    let stats = HistogramStats::of(snapshot);
    write_line(output, &format!("               min = {} {unit}", snapshot.min()), line_separator)?;
    write_line(output, &format!("               max = {} {unit}", snapshot.max()), line_separator)?;
    write_line(output, &format!("              mean = {} {unit}", format_fixed(stats.mean, 2, 2)), line_separator)?;
    write_line(output, &format!("            stddev = {}", format_fixed(stats.stddev, 2, 2)), line_separator)?;
    for (label, percentile) in [("75%", 75.0), ("95%", 95.0), ("98%", 98.0), ("99%", 99.0), ("99.9%", 99.9)] {
      let value = stats.value_at_percentile(percentile);
      write_line(output, &format!("            {label:>5} <= {value} {unit}"), line_separator)?;
    }
    if verbose {
      write_distribution(snapshot, &stats, output)?;
    }
  }
  write_line(output, "", line_separator)
}

/// The mean, the standard deviation and the recorded buckets of a
/// histogram snapshot, computed from its recorded buckets as the original
/// computes them.
struct HistogramStats {
  total: u64,
  mean: f64,
  stddev: f64,
  recorded: Vec<(u64, u64)>,
}

impl HistogramStats {
  fn of(snapshot: &hdrhistogram::Histogram<u64>) -> Self {
    let recorded: Vec<(u64, u64)> =
      snapshot.iter_recorded().map(|step| (step.value_iterated_to(), step.count_at_value())).collect();
    let total = snapshot.len();
    if total == 0 {
      return Self { total, mean: 0.0, stddev: 0.0, recorded };
    }
    let mean =
      recorded.iter().map(|(value, count)| snapshot.median_equivalent(*value) as f64 * *count as f64).sum::<f64>()
        / total as f64;
    let variance = recorded
      .iter()
      .map(|(value, count)| {
        let deviation = snapshot.median_equivalent(*value) as f64 - mean;
        deviation * deviation * *count as f64
      })
      .sum::<f64>()
      / total as f64;
    Self { total, mean, stddev: variance.sqrt(), recorded }
  }

  /// The value at `percentile`: the upper end of the first bucket whose
  /// cumulative count reaches the count the percentile asks for.
  fn value_at_percentile(&self, percentile: f64) -> u64 {
    if self.total == 0 {
      return 0;
    }
    let needed = (percentile.next_down().clamp(0.0, 100.0) * self.total as f64 / 100.0).ceil().max(1.0) as u64;
    let mut cumulative = 0_u64;
    for (value, count) in &self.recorded {
      cumulative += count;
      if cumulative >= needed {
        return *value;
      }
    }
    0
  }
}

/// Writes the percentile distribution of a histogram. Its lines always
/// end with `\n`, whatever the line separator of the report is.
fn write_distribution(
  snapshot: &hdrhistogram::Histogram<u64>,
  stats: &HistogramStats,
  output: &mut dyn Write,
) -> io::Result<()> {
  let header = format!("{:>12} {:>14} {:>10} {:>14}", "Value", "Percentile", "TotalCount", "1/(1-Percentile)");
  write_line(output, &header, "\n")?;
  write_line(output, "", "\n")?;
  if stats.total > 0 {
    let mut cumulative = 0_u64;
    let mut percentile = 0.0_f64;
    let mut last_value = 0_u64;
    for (value, count) in &stats.recorded {
      cumulative += count;
      last_value = *value;
      let cumulative_percent = 100.0 * cumulative as f64 / stats.total as f64;
      let is_last = cumulative == stats.total;
      while cumulative_percent >= percentile {
        let fraction = percentile / 100.0;
        let reciprocal = format_fixed(1.0 / (1.0 - fraction), 2, 14);
        let row = format!(
          "{} {} {:>10} {}",
          format_fixed(*value as f64, snapshot.sigfig() as usize, 12),
          format_fixed(fraction, 12, 2),
          cumulative,
          reciprocal
        );
        write_line(output, &row, "\n")?;
        if is_last {
          break;
        }
        percentile += percentile_increment(percentile);
      }
      if is_last {
        break;
      }
    }
    let row = format!(
      "{} {} {:>10}",
      format_fixed(last_value as f64, snapshot.sigfig() as usize, 12),
      format_fixed(1.0, 12, 2),
      stats.total
    );
    write_line(output, &row, "\n")?;
  }
  let mean = format_fixed(stats.mean, snapshot.sigfig() as usize, 12);
  let stddev = format_fixed(stats.stddev, snapshot.sigfig() as usize, 12);
  write_line(output, &format!("#[Mean    = {mean}, StdDeviation   = {stddev}]"), "\n")?;
  let max = format_fixed(snapshot.max() as f64, snapshot.sigfig() as usize, 12);
  write_line(output, &format!("#[Max     = {max}, Total count    = {:>12}]", stats.total), "\n")?;
  let sub_buckets = sub_bucket_count(snapshot.sigfig());
  write_line(output, &format!("#[Buckets = {:>12}, SubBuckets     = {:>12}]", snapshot.buckets(), sub_buckets), "\n")
}

/// The step from one iterated percentile level to the next, in percent.
fn percentile_increment(percentile: f64) -> f64 {
  let exponent = (100.0 / (100.0 - percentile)).log2().floor() as i32;
  100.0 / (5.0 * 2.0_f64.powi(exponent + 1))
}

/// The number of sub buckets of a histogram with `sigfig` significant
/// digits: the smallest power of two that keeps single unit resolution
/// up to twice ten to the power of `sigfig`.
fn sub_bucket_count(sigfig: u8) -> u32 {
  let target = 2 * 10_u32.pow(u32::from(sigfig));
  let mut count = 1_u32;
  while count < target {
    count *= 2;
  }
  count
}

/// A signed nanosecond count as the original's pretty duration: a
/// non-negative count chooses its unit, a negative count stays in
/// nanoseconds with its magnitude formatted to four significant digits.
fn pretty_signed_nanos(nanos: i64) -> String {
  if nanos >= 0 {
    PrettyDurationInput::Finite(Duration::from_nanos(nanos as u64)).pretty()
  } else {
    format!("-{} ns", format_g((nanos as f64).abs(), 4))
  }
}

/// The shortest decimal digits of a positive finite value and the decimal
/// exponent of its first digit, taken from the shortest representation.
fn shortest_digits(value: f64) -> (Vec<u8>, i32) {
  let scientific = format!("{value:e}");
  let (mantissa, exponent_text) = scientific.split_once('e').expect("LowerExp always writes an exponent");
  let exponent: i32 = exponent_text.parse().expect("the exponent is an integer");
  let digits: Vec<u8> = mantissa.bytes().filter(|byte| *byte != b'.').collect();
  (digits, exponent)
}

/// Formats a value as the original's `Double.toString` does: plain
/// decimal inside the range from a thousandth to ten million, scientific
/// notation outside it, and always a fraction digit.
pub(crate) fn java_double_to_string(value: f64) -> String {
  if value.is_nan() {
    return "NaN".to_owned();
  }
  if value == f64::INFINITY {
    return "Infinity".to_owned();
  }
  if value == f64::NEG_INFINITY {
    return "-Infinity".to_owned();
  }
  let sign = if value.is_sign_negative() { "-" } else { "" };
  let magnitude = value.abs();
  if magnitude == 0.0 {
    return format!("{sign}0.0");
  }
  let (digits, exponent) = shortest_digits(magnitude);
  let digits_text = core::str::from_utf8(&digits).expect("digits are ASCII");
  if (1.0e-3..1.0e7).contains(&magnitude) {
    let point = exponent + 1;
    if point <= 0 {
      format!("{sign}0.{}{digits_text}", "0".repeat((-point) as usize))
    } else if point as usize >= digits.len() {
      format!("{sign}{digits_text}{}.0", "0".repeat(point as usize - digits.len()))
    } else {
      let point = point as usize;
      format!("{sign}{}.{}", &digits_text[..point], &digits_text[point..])
    }
  } else {
    let rest = if digits.len() > 1 { &digits_text[1..] } else { "0" };
    format!("{sign}{}.{rest}E{exponent}", &digits_text[..1])
  }
}

/// Increments a string of decimal digits by one, growing it on overflow.
fn increment_digits(digits: &mut Vec<u8>) {
  let mut index = digits.len();
  loop {
    if index == 0 {
      digits.insert(0, b'1');
      return;
    }
    index -= 1;
    if digits[index] == b'9' {
      digits[index] = b'0';
    } else {
      digits[index] += 1;
      return;
    }
  }
}

/// Formats a value with `decimals` fraction digits, rounding the shortest
/// decimal digits up on a half, right aligned in `width`. The spellings
/// of the special values are those of the original's formatter.
pub(crate) fn format_fixed(value: f64, decimals: usize, width: usize) -> String {
  let text = format_fixed_text(value, decimals);
  if text.len() >= width {
    text
  } else {
    format!("{}{text}", " ".repeat(width - text.len()))
  }
}

fn format_fixed_text(value: f64, decimals: usize) -> String {
  if value.is_nan() {
    return "NaN".to_owned();
  }
  if value == f64::INFINITY {
    return "Infinity".to_owned();
  }
  if value == f64::NEG_INFINITY {
    return "-Infinity".to_owned();
  }
  let sign = if value.is_sign_negative() { "-" } else { "" };
  let magnitude = value.abs();
  // The rounded value scaled by ten to the power of `decimals`, as a
  // string of decimal digits.
  let scaled: Vec<u8> = if magnitude == 0.0 {
    vec![b'0']
  } else {
    let (digits, exponent) = shortest_digits(magnitude);
    let shift = exponent - digits.len() as i32 + 1 + decimals as i32;
    if shift >= 0 {
      let mut scaled = digits;
      scaled.extend(core::iter::repeat_n(b'0', shift as usize));
      scaled
    } else {
      let drop = (-shift) as usize;
      if drop > digits.len() {
        vec![b'0']
      } else if drop == digits.len() {
        if digits[0] >= b'5' {
          vec![b'1']
        } else {
          vec![b'0']
        }
      } else {
        let mut kept = digits[..digits.len() - drop].to_vec();
        if digits[digits.len() - drop] >= b'5' {
          increment_digits(&mut kept);
        }
        kept
      }
    }
  };
  let scaled_text = core::str::from_utf8(&scaled).expect("digits are ASCII");
  if decimals == 0 {
    return format!("{sign}{scaled_text}");
  }
  if scaled_text.len() > decimals {
    let point = scaled_text.len() - decimals;
    format!("{sign}{}.{}", &scaled_text[..point], &scaled_text[point..])
  } else {
    format!("{sign}0.{}{scaled_text}", "0".repeat(decimals - scaled_text.len()))
  }
}

/// Formats a non-negative finite value like the original's `%g` with
/// `precision` significant digits, rounding the shortest decimal digits
/// up on a half.
fn format_g(value: f64, precision: usize) -> String {
  if value == 0.0 {
    let digits = vec![b'0'; precision];
    return render_g(&digits, 0, precision);
  }
  let (mut digits, mut exponent) = shortest_digits(value);
  if digits.len() > precision {
    let dropped = digits[precision];
    digits.truncate(precision);
    if dropped >= b'5' {
      let mut index = precision;
      loop {
        if index == 0 {
          digits.insert(0, b'1');
          digits.truncate(precision);
          exponent += 1;
          break;
        }
        index -= 1;
        if digits[index] == b'9' {
          digits[index] = b'0';
        } else {
          digits[index] += 1;
          break;
        }
      }
    }
  } else {
    digits.resize(precision, b'0');
  }
  render_g(&digits, exponent, precision)
}

/// Renders `precision` significant digits whose first digit has decimal
/// exponent `exponent`, in the `%g` form.
fn render_g(digits: &[u8], exponent: i32, precision: usize) -> String {
  let digits_text = core::str::from_utf8(digits).expect("digits are ASCII");
  if exponent < -4 || exponent >= precision as i32 {
    let mut out = String::new();
    out.push(digits_text.as_bytes()[0] as char);
    if precision > 1 {
      out.push('.');
      out.push_str(&digits_text[1..]);
    }
    out.push('e');
    out.push(if exponent < 0 { '-' } else { '+' });
    let magnitude = exponent.unsigned_abs();
    if magnitude < 10 {
      out.push('0');
    }
    out.push_str(&format!("{magnitude}"));
    return out;
  }
  let point = exponent + 1;
  if point <= 0 {
    let mut out = String::from("0.");
    for _ in 0..(-point) {
      out.push('0');
    }
    out.push_str(digits_text);
    out
  } else if point as usize >= precision {
    let mut out = String::from(digits_text);
    for _ in 0..(point as usize - precision) {
      out.push('0');
    }
    out
  } else {
    let point = point as usize;
    let mut out = String::from(&digits_text[..point]);
    out.push('.');
    out.push_str(&digits_text[point..]);
    out
  }
}
