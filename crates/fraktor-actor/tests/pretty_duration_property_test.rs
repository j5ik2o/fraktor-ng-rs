// Property tests of the pretty duration formatting outside the original
// PrettyDurationSpec (design of #186, docs/design/issue-186-util-specs.md
// §3, D1 and the round-trip part of D2). The rounding mode itself is fixed
// by the fixed cases in pretty_duration_format_test.rs, not here. These are
// not ports of Pekko cases and are not counted in the ledger. Inputs come
// from a fixed-seed SplitMix64, so a failure repeats.

use core::time::Duration;

use fraktor_actor::kernel::pretty_duration::PrettyDurationInput;

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }
}

fn unit_rank_and_scale(text: &str) -> (u8, f64) {
  let (value, unit) = text.split_once(' ').expect("formatted duration has a unit");
  let parsed: f64 = value.parse().expect("formatted value is a number");
  let (rank, scale) = match unit {
    | "ns" => (0, 1.0),
    | "μs" => (1, 1_000.0),
    | "ms" => (2, 1_000_000.0),
    | "s" => (3, 1_000_000_000.0),
    | "min" => (4, 60_000_000_000.0),
    | "h" => (5, 3_600_000_000_000.0),
    | "d" => (6, 86_400_000_000_000.0),
    | other => panic!("unknown unit {other}"),
  };
  (rank, parsed * scale)
}

#[test]
fn pretty_duration_unit_never_goes_back_as_duration_grows() {
  let mut rng = SplitMix64 { state: 0x186 };
  let mut samples: Vec<u64> = (0..2000).map(|_| rng.next_u64() % (1 << 62)).collect();
  samples.sort_unstable();
  let mut highest_rank = 0;
  for nanos in samples {
    let text = PrettyDurationInput::Finite(Duration::from_nanos(nanos)).pretty();
    let (rank, _) = unit_rank_and_scale(&text);
    assert!(rank >= highest_rank, "unit went back at {nanos} ns: {text}");
    highest_rank = rank;
  }
}

#[test]
fn pretty_duration_displayed_value_round_trips_within_rounding() {
  let mut rng = SplitMix64 { state: 0x1862 };
  for case in 0..2000 {
    let nanos = rng.next_u64() % (1 << 62);
    if nanos == 0 {
      continue;
    }
    let text = PrettyDurationInput::Finite(Duration::from_nanos(nanos)).pretty();
    let (_, back) = unit_rank_and_scale(&text);
    let relative = (back - nanos as f64).abs() / nanos as f64;
    // Four significant digits: half of the last digit is 0.05% at the top of
    // a decade and 0.5% at the bottom; the bound keeps margin over both.
    assert!(relative < 0.006, "case {case}: {nanos} ns formatted as {text}, back to {back}");
  }
}

#[test]
fn pretty_duration_unit_boundaries() {
  let cases: [(u64, &str); 12] = [
    (999, "ns"),
    (1_000, "μs"),
    (999_999, "μs"),
    (1_000_000, "ms"),
    (999_999_999, "ms"),
    (1_000_000_000, "s"),
    (59_999_999_999, "s"),
    (60_000_000_000, "min"),
    (3_599_999_999_999, "min"),
    (3_600_000_000_000, "h"),
    (86_399_999_999_999, "h"),
    (86_400_000_000_000, "d"),
  ];
  for (nanos, unit) in cases {
    let text = PrettyDurationInput::Finite(Duration::from_nanos(nanos)).pretty();
    assert!(text.ends_with(unit), "{nanos} ns formatted as {text}, expected unit {unit}");
  }
}
