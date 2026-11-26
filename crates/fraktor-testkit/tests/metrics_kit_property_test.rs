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

// Property-based tests of the metrics kit (the design of #183,
// docs/design/issue-183-metrics-kit.md §5, P1 to P6).
//
// These are not ports of Pekko cases and are not counted in the ledger. The
// inputs come from a fixed-seed SplitMix64, so a failure repeats; every
// failure message names the seed. No input is left out: empty keys, dots in
// runs, brackets, tabs, newlines, ideographic spaces, the extremes of i64,
// zero and negative operation counts, every number of significant digits,
// and histogram values on both sides of every bound are all drawn.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use fraktor_testkit::metrics_kit::{
  AveragingGauge, HdrHistogram, KnownOpsInTimespanTimer, Metric, MetricKey, MetricsError, MetricsKit,
  MetricsKitSettings,
};

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn new(seed: u64) -> Self {
    Self { state: seed }
  }

  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: usize) -> usize {
    (self.next_u64() % bound as u64) as usize
  }

  fn pick<T: Copy>(&mut self, items: &[T]) -> T {
    items[self.below(items.len())]
  }

  /// A value in `low..=high`.
  fn in_range(&mut self, low: i64, high: i64) -> i64 {
    assert!(low <= high);
    let span = (i128::from(high) - i128::from(low)) as u128 + 1;
    let offset = (u128::from(self.next_u64()) << 64 | u128::from(self.next_u64())) % span;
    (i128::from(low) + offset as i128) as i64
  }

  fn i64(&mut self) -> i64 {
    match self.below(6) {
      | 0 => self.pick(&[i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX]),
      | 1 => self.next_u64() as i64,
      | _ => self.in_range(-1_000, 1_000),
    }
  }
}

const SEEDS: u64 = 64;

fn seeds() -> impl Iterator<Item = u64> {
  (0..SEEDS).map(|n| 0x183_0000 + n)
}

fn ok<T>(result: Result<T, MetricsError>, seed: u64) -> T {
  result.unwrap_or_else(|error| panic!("seed {seed}: unexpected metrics error: {error}"))
}

fn same_f64(left: f64, right: f64) -> bool {
  (left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits()
}

// --- P1: keys -------------------------------------------------------------------

// Pieces of key parts: every character the original replaces, the runs of
// dots it treats in order, and whitespace it leaves alone.
const PIECES: [&str; 24] = [
  "a", "z", "0", "-", "|", "_", ".", "..", "...", "....", " ", "/", "[", "]", "(", ")", "<", ">", "\t", "\n",
  "\u{3000}", "\u{2026}", "日", "😀",
];

fn random_part(random: &mut SplitMix64) -> String {
  match random.below(10) {
    | 0 => String::new(),
    | 1 => (0..random.below(300) + 100).map(|_| random.pick(&PIECES)).collect(),
    | _ => (0..random.below(8) + 1).map(|_| random.pick(&PIECES)).collect(),
  }
}

/// `MetricKey.sanitizeMetricKeyPart`, in the original's order.
fn sanitize(part: &str) -> String {
  part
    .replace("...", "\u{2026}")
    .replace('.', "-")
    .replace(['[', ']', '(', ')', '<', '>'], "|")
    .replace([' ', '/'], "-")
}

#[test]
fn p1_a_key_of_one_part_is_the_sanitized_part() {
  for seed in seeds() {
    let mut random = SplitMix64::new(seed);
    for _ in 0..64 {
      let part = random_part(&mut random);
      let key = MetricKey::from_string(&part);
      assert_eq!(key.as_str(), sanitize(&part), "seed {seed}: from_string({part:?})");
      assert_eq!(key.to_string(), key.as_str(), "seed {seed}: Display is the path");
      for forbidden in ['.', ' ', '/', '[', ']', '(', ')', '<', '>'] {
        assert!(!key.as_str().contains(forbidden), "seed {seed}: {:?} contains {forbidden:?}", key.as_str());
      }
      let again = MetricKey::from_string(key.as_str());
      assert_eq!(again.as_str(), key.as_str(), "seed {seed}: sanitizing is idempotent for {part:?}");
    }
  }
}

#[test]
fn p1_a_child_key_joins_sanitized_parts_with_a_dot() {
  for seed in seeds() {
    let mut random = SplitMix64::new(seed);
    for _ in 0..32 {
      let parts: Vec<String> = (0..random.below(5) + 1).map(|_| random_part(&mut random)).collect();
      let mut key = MetricKey::from_string(&parts[0]);
      for part in &parts[1..] {
        key = key.child(part);
      }
      let expected = parts.iter().map(|part| sanitize(part)).collect::<Vec<_>>().join(".");
      assert_eq!(key.as_str(), expected, "seed {seed}: parts {parts:?}");
      assert_eq!(key.to_string(), expected, "seed {seed}: Display is the path");
    }
  }
}

#[test]
fn p1_the_documented_edge_keys() {
  assert_eq!(MetricKey::from_string("").as_str(), "");
  assert_eq!(MetricKey::from_string("x").child("").as_str(), "x.");
  assert_eq!(MetricKey::from_string("..").as_str(), "--");
  assert_eq!(MetricKey::from_string("....").as_str(), "\u{2026}-");
  assert_eq!(MetricKey::from_string("a\tb\nc\u{3000}d").as_str(), "a\tb\nc\u{3000}d");
  assert_eq!(MetricKey::from_string("a b/c[d](e)<f>").as_str(), "a-b-c|d||e||f|");
  assert_eq!(MetricKey::from_string("metrics-kit").child("file-desc").as_str(), "metrics-kit.file-desc");
}

// --- P2: averaging gauge ---------------------------------------------------------

fn reference_average(values: &[i64]) -> f64 {
  values.iter().copied().fold(0_i64, i64::wrapping_add) as f64 / values.len() as f64
}

#[test]
fn p2_bulk_and_single_adds_agree_with_the_wrapping_mean() {
  for seed in seeds() {
    let mut random = SplitMix64::new(seed);
    for _ in 0..32 {
      let values: Vec<i64> = (0..random.below(40)).map(|_| random.i64()).collect();
      let expected = reference_average(&values);

      let single = AveragingGauge::new();
      for value in &values {
        single.add(*value);
      }
      let bulk = AveragingGauge::default();
      bulk.add_slice(&values);
      let mixed = AveragingGauge::new();
      let shared = mixed.clone();
      let mut rest = values.as_slice();
      while !rest.is_empty() {
        let take = random.below(rest.len()) + 1;
        if take == 1 && random.below(2) == 0 {
          shared.add(rest[0]);
        } else {
          mixed.add_slice(&rest[..take]);
        }
        rest = &rest[take..];
      }

      for (name, gauge) in [("single", &single), ("bulk", &bulk), ("mixed", &mixed), ("clone", &shared)] {
        assert!(
          same_f64(gauge.value(), expected),
          "seed {seed}: {name} value {} differs from {expected} for {values:?}",
          gauge.value()
        );
      }
      if values.is_empty() {
        assert!(single.value().is_nan(), "seed {seed}: an empty gauge is NaN");
      }
    }
  }
}

// --- P3: histogram ---------------------------------------------------------------

/// The layout of a Java `org.HdrHistogram.Histogram` with the lowest
/// discernible value 1 (unit magnitude 0), the one the original builds:
/// `AbstractHistogram.init`, `getBucketsNeededToCoverValue`,
/// `countsArrayIndex`, `lowestEquivalentValue` and
/// `sizeOfEquivalentValueRange`.
struct ReferenceLayout {
  sub_bucket_count: u64,
  sub_bucket_half_count_magnitude: u32,
  bucket_count: u32,
}

impl ReferenceLayout {
  fn new(highest_trackable_value: i64, digits: u8) -> Self {
    let largest_value_with_single_unit_resolution = 2 * 10_u64.pow(u32::from(digits));
    let sub_bucket_count_magnitude = 64 - (largest_value_with_single_unit_resolution - 1).leading_zeros();
    let sub_bucket_half_count_magnitude = sub_bucket_count_magnitude.max(1) - 1;
    let sub_bucket_count = 1_u64 << (sub_bucket_half_count_magnitude + 1);
    let mut smallest_untrackable_value = sub_bucket_count;
    let mut bucket_count = 1;
    while smallest_untrackable_value <= highest_trackable_value as u64 {
      if smallest_untrackable_value > (i64::MAX as u64) / 2 {
        bucket_count += 1;
        break;
      }
      smallest_untrackable_value <<= 1;
      bucket_count += 1;
    }
    Self { sub_bucket_count, sub_bucket_half_count_magnitude, bucket_count }
  }

  /// The highest value whose counts index is inside the counts array: the
  /// top of the last bucket, not the highest trackable value.
  fn highest_recordable(&self) -> i64 {
    let top = (u128::from(self.sub_bucket_count) << (self.bucket_count - 1)) - 1;
    i64::try_from(top).unwrap_or(i64::MAX)
  }

  fn bucket_index(&self, value: u64) -> u32 {
    let leading_zero_count_base = 63 - self.sub_bucket_half_count_magnitude;
    leading_zero_count_base - (value | (self.sub_bucket_count - 1)).leading_zeros()
  }

  /// `lowestEquivalentValue(value)..=highestEquivalentValue(value)`.
  fn equivalent_range(&self, value: u64) -> (u128, u128) {
    let bucket = self.bucket_index(value);
    let sub_bucket = value >> bucket;
    let lowest = u128::from(sub_bucket) << bucket;
    let size_magnitude = if sub_bucket >= self.sub_bucket_count { bucket + 1 } else { bucket };
    (lowest, lowest + (1_u128 << size_magnitude) - 1)
  }

  fn holds(&self, value: u64, reported: u64) -> bool {
    let (lowest, highest) = self.equivalent_range(value);
    lowest <= u128::from(reported) && u128::from(reported) <= highest
  }
}

#[test]
fn p3_the_reference_layout_matches_the_documented_bound() {
  assert_eq!(ReferenceLayout::new(100_000_000, 4).highest_recordable(), 134_217_727);
  assert_eq!(ReferenceLayout::new(i64::MAX, 4).highest_recordable(), i64::MAX);
}

fn random_highest_trackable_value(random: &mut SplitMix64) -> i64 {
  match random.below(6) {
    | 0 => random.in_range(2, 100),
    | 1 => random.in_range(2, 1 << 20),
    | 2 => random.in_range(2, 1 << 40),
    | 3 => random.in_range(1 << 62, i64::MAX),
    | 4 => i64::MAX,
    | _ => 100_000_000,
  }
}

fn random_value(random: &mut SplitMix64, highest: i64, bound: i64) -> i64 {
  match random.below(10) {
    | 0 => random.pick(&[0, 1, highest, bound, bound.saturating_add(1)]),
    | 1 | 2 => random.in_range(0, highest),
    | 3 if bound > highest => random.in_range(highest + 1, bound),
    | 4 if bound < i64::MAX => random.in_range(bound + 1, i64::MAX),
    | 5 => random.in_range(i64::MIN, -1),
    | 6 => random.pick(&[i64::MIN, -1]),
    | _ => random.in_range(0, highest.min(1_000)),
  }
}

#[test]
fn p3_count_min_and_max_follow_the_recordable_values() {
  for seed in seeds() {
    let mut random = SplitMix64::new(seed);
    for _ in 0..4 {
      let highest = random_highest_trackable_value(&mut random);
      let digits = random.below(6) as u8;
      let layout = ReferenceLayout::new(highest, digits);
      let bound = layout.highest_recordable();
      let histogram = ok(HdrHistogram::new(highest, digits, "ns"), seed);
      let context = format!("seed {seed}: highest {highest}, digits {digits}, bound {bound}");

      let mut recorded: Vec<u64> = Vec::new();
      for _ in 0..random.below(40) {
        let value = random_value(&mut random, highest, bound);
        let result = histogram.update(value);
        let recordable = (0..=bound).contains(&value);
        match result {
          | Ok(()) => {
            assert!(recordable, "{context}: {value} was recorded");
            recorded.push(value as u64);
          },
          | Err(error) => {
            assert!(!recordable, "{context}: {value} failed with {error}");
            assert!(error.to_string().contains("can not be stored in this histogram"), "{context}: {error}");
          },
        }
        assert_eq!(histogram.count(), recorded.len() as u64, "{context}: count after {value}");
      }

      match (recorded.iter().min(), recorded.iter().max()) {
        | (Some(&low), Some(&high)) => {
          assert!(layout.holds(low, histogram.min()), "{context}: min {} for {low}", histogram.min());
          assert!(layout.holds(high, histogram.max()), "{context}: max {} for {high}", histogram.max());
        },
        | _ => {
          assert_eq!(histogram.min(), 0, "{context}: min of an empty histogram");
          assert_eq!(histogram.max(), 0, "{context}: max of an empty histogram");
        },
      }
    }
  }
}

// --- P4: registry ----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
  AveragingGauge,
  HdrHistogram,
  KnownOpsInTimespanTimer,
}

fn kind_of(metric: &Metric) -> Option<Kind> {
  if matches!(metric, Metric::AveragingGauge(_)) {
    Some(Kind::AveragingGauge)
  } else if matches!(metric, Metric::HdrHistogram(_)) {
    Some(Kind::HdrHistogram)
  } else if matches!(metric, Metric::KnownOpsInTimespanTimer(_)) {
    Some(Kind::KnownOpsInTimespanTimer)
  } else {
    None
  }
}

enum Entry {
  Gauge(AveragingGauge, Vec<i64>),
  Histogram(HdrHistogram, u64),
  Timer(i64),
}

impl Entry {
  fn kind(&self) -> Kind {
    match self {
      | Entry::Gauge(..) => Kind::AveragingGauge,
      | Entry::Histogram(..) => Kind::HdrHistogram,
      | Entry::Timer(_) => Kind::KnownOpsInTimespanTimer,
    }
  }
}

// Keys whose derived names collide across kinds: the histogram under "a"
// registers "a.hdr-histogram", where a gauge or timer of the second key goes.
const KEY_PARTS: [&[&str]; 5] =
  [&["a"], &["a", "hdr-histogram"], &["b"], &["b", "hdr-histogram"], &["a", "hdr-histogram", "hdr-histogram"]];

fn key_of(parts: &[&str]) -> MetricKey {
  let mut key = MetricKey::from_string(parts[0]);
  for part in &parts[1..] {
    key = key.child(part);
  }
  key
}

fn assert_registry_matches(kit: &MetricsKit, model: &BTreeMap<String, Entry>, context: &str) {
  let registered = kit.registered_metrics();
  let actual: Vec<(&String, Option<Kind>)> = registered.iter().map(|(key, metric)| (key, kind_of(metric))).collect();
  let expected: Vec<(&String, Option<Kind>)> = model.iter().map(|(key, entry)| (key, Some(entry.kind()))).collect();
  assert_eq!(actual, expected, "{context}: registry");
}

fn assert_kind_conflict<T>(result: Result<T, MetricsError>, context: &str) {
  match result {
    | Err(MetricsError::KindConflict { .. }) => {},
    | Err(error) => panic!("{context}: expected a kind conflict, got {error}"),
    | Ok(_) => panic!("{context}: expected a kind conflict, got a metric"),
  }
}

#[test]
fn p4_same_kind_shares_state_other_kinds_conflict_and_clear_starts_afresh() {
  for seed in seeds() {
    let mut random = SplitMix64::new(seed);
    let kit = MetricsKit::new(MetricsKitSettings::default());
    let mut model: BTreeMap<String, Entry> = BTreeMap::new();
    for step in 0..60 {
      let parts = random.pick(&KEY_PARTS);
      let key = key_of(parts);
      let name = parts.join(".");
      let context = format!("seed {seed}, step {step}, key {name}");
      match random.below(7) {
        | 0 | 1 => {
          let value = random.in_range(-100, 100);
          let result = kit.average_gauge(&key);
          match model.get_mut(&name) {
            | None => {
              let gauge = ok(result, seed);
              assert!(gauge.value().is_nan(), "{context}: a new gauge is empty");
              gauge.add(value);
              model.insert(name, Entry::Gauge(gauge, vec![value]));
            },
            | Some(Entry::Gauge(first, values)) => {
              let gauge = ok(result, seed);
              gauge.add(value);
              values.push(value);
              let expected = reference_average(values);
              assert!(same_f64(first.value(), expected), "{context}: the first handle sees the add");
              assert!(same_f64(gauge.value(), expected), "{context}: the new handle sees every add");
            },
            | Some(_) => assert_kind_conflict(result, &context),
          }
        },
        | 2 | 3 => {
          let name = format!("{name}.hdr-histogram");
          let result = kit.hdr_histogram(&key, 100_000_000, 4, "ns");
          match model.get_mut(&name) {
            | None => {
              let histogram = ok(result, seed);
              assert_eq!(histogram.count(), 0, "{context}: a new histogram is empty");
              ok(histogram.update(random.in_range(0, 1_000)), seed);
              model.insert(name, Entry::Histogram(histogram, 1));
            },
            | Some(Entry::Histogram(first, count)) => {
              let histogram = ok(result, seed);
              ok(histogram.update(random.in_range(0, 1_000)), seed);
              *count += 1;
              assert_eq!(first.count(), *count, "{context}: the first handle sees the update");
              assert_eq!(histogram.count(), *count, "{context}: the new handle sees every update");
            },
            | Some(_) => assert_kind_conflict(result, &context),
          }
        },
        | 4 | 5 => {
          let ops = random.in_range(-5, 50);
          let ran = AtomicBool::new(false);
          let result = kit.timed_with_known_ops(&key, ops, || {
            ran.store(true, Ordering::SeqCst);
            step
          });
          let registered_ops = match model.get(&name) {
            | None => Some(ops),
            | Some(Entry::Timer(first_ops)) => Some(*first_ops),
            | Some(_) => None,
          };
          match registered_ops {
            | Some(registered_ops) => {
              assert_eq!(ok(result, seed), step, "{context}: the block's value");
              assert!(ran.load(Ordering::SeqCst), "{context}: the block ran");
              model.entry(name.clone()).or_insert(Entry::Timer(ops));
              let registered = kit.registered_metrics();
              let Some(Metric::KnownOpsInTimespanTimer(timer)) = registered.get(&name) else {
                panic!("{context}: no timer is registered");
              };
              assert_eq!(timer.count(), registered_ops, "{context}: the first timer stays registered");
              assert!(!timer.stop(), "{context}: the timer is stopped");
            },
            | None => {
              assert_kind_conflict(result, &context);
              // The original's block is a by-name argument, evaluated only
              // after `getOrRegister` returns.
              assert!(!ran.load(Ordering::SeqCst), "{context}: the block ran despite the conflict");
            },
          }
        },
        | _ => {
          kit.clear_metrics();
          model.clear();
        },
      }
      assert_registry_matches(&kit, &model, &context);
    }
  }
}

// --- P5: timer -------------------------------------------------------------------

#[test]
fn p5_stop_succeeds_at_most_once_and_the_count_never_changes() {
  for seed in seeds() {
    let mut random = SplitMix64::new(seed);
    for round in 0..8 {
      let ops = random.i64();
      let context = format!("seed {seed}, round {round}, ops {ops}");
      let timer = KnownOpsInTimespanTimer::new(ops);
      assert_eq!(timer.count(), ops, "{context}: count before stop");
      assert!(timer.elapsed().is_none(), "{context}: elapsed before stop");
      assert!(timer.avg_duration_nanos().is_none(), "{context}: avg_duration_nanos before stop");
      assert!(timer.ops_per_second().is_none(), "{context}: ops_per_second before stop");

      let calls = random.below(9);
      let results: Vec<bool> = if random.below(2) == 0 {
        (0..calls).map(|_| timer.stop()).collect()
      } else {
        thread::scope(|scope| {
          let handles: Vec<_> = (0..calls)
            .map(|_| {
              let timer = timer.clone();
              scope.spawn(move || timer.stop())
            })
            .collect();
          handles.into_iter().map(|handle| handle.join().expect("stop does not panic")).collect()
        })
      };

      let successes = results.iter().filter(|stopped| **stopped).count();
      assert_eq!(successes, usize::from(calls > 0), "{context}: {calls} calls returned {results:?}");
      assert_eq!(timer.count(), ops, "{context}: count after stop");
      if calls == 0 {
        assert!(timer.elapsed().is_none(), "{context}: elapsed without a stop");
        continue;
      }
      let elapsed = timer.elapsed();
      assert!(elapsed.is_some(), "{context}: elapsed after stop");
      assert!(timer.avg_duration_nanos().is_some(), "{context}: avg_duration_nanos after stop");
      assert!(timer.ops_per_second().is_some(), "{context}: ops_per_second after stop");
      assert!(!timer.stop(), "{context}: a later stop");
      assert_eq!(timer.elapsed(), elapsed, "{context}: a later stop keeps the stop time");
    }
  }
}

// --- P6: file descriptor keys ----------------------------------------------------

const FILE_DESCRIPTOR_NAMES: [&str; 3] = ["file-descriptors-max", "file-descriptors-open", "file-descriptors-ratio"];

#[test]
fn p6_measuring_file_descriptors_registers_exactly_the_three_derived_keys() {
  for seed in seeds() {
    let mut random = SplitMix64::new(seed);
    for _ in 0..4 {
      let parts: Vec<String> = (0..random.below(3) + 1).map(|_| random_part(&mut random)).collect();
      let mut key = MetricKey::from_string(&parts[0]);
      for part in &parts[1..] {
        key = key.child(part);
      }
      let prefix = parts.iter().map(|part| sanitize(part)).collect::<Vec<_>>().join(".");
      let context = format!("seed {seed}: parts {parts:?}");

      let kit = MetricsKit::new(MetricsKitSettings::default());
      for n in 0..random.below(4) {
        ok(kit.average_gauge(&MetricKey::from_string(&format!("unrelated-{n}"))), seed);
      }
      let before: BTreeSet<String> = kit.registered_metrics().into_keys().collect();

      ok(kit.measure_file_descriptors(&key), seed);

      let after: BTreeSet<String> = kit.registered_metrics().into_keys().collect();
      let added: Vec<String> = after.difference(&before).cloned().collect();
      let expected: Vec<String> =
        FILE_DESCRIPTOR_NAMES.iter().map(|name| format!("{prefix}.file-descriptors.{name}")).collect();
      assert_eq!(added, expected, "{context}: added keys");
      assert!(before.is_subset(&after), "{context}: the earlier keys stay");
    }
  }
}

#[test]
fn p6_the_spec_key_registers_the_original_names() {
  let kit = MetricsKit::new(MetricsKitSettings::default());

  ok(kit.measure_file_descriptors(&MetricKey::from_string("metrics-kit").child("file-desc")), 0);

  let keys: Vec<String> = kit.registered_metrics().into_keys().collect();
  assert_eq!(
    keys,
    [
      "metrics-kit.file-desc.file-descriptors.file-descriptors-max",
      "metrics-kit.file-desc.file-descriptors.file-descriptors-open",
      "metrics-kit.file-desc.file-descriptors.file-descriptors-ratio",
    ]
  );
}
