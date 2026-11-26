#!/bin/bash
# The alternating speed measurement of design section 7.5 for stage 1 of
# #234 (issue #249): runs the ignored bench test of
# crates/fraktor-actor/tests/actor_system_send_stage1_bench.rs in the
# checkouts given as arguments, in the order A, B, B, A, A, B, B, A.
#
# Usage: scripts/stage1-bench.sh <checkout A> <checkout B> [checkout A0]
#
# A is the main commit just before the stage, B the stage's commit, and
# the optional A0 the pre-stage-0 commit (7ecb3f5) the design also tracks.
# The bench test itself arrives with the stage, so A and A0 do not have
# it: when a comparison checkout lacks the file, this script copies it
# from the B checkout first. The copy is the only change made to those
# checkouts, and it is test-only code the measured library never sees.
#
# A pair whose control loop values drift by more than 5 percent is
# discarded as machine noise. When fewer than four pairs survive, the
# whole sequence is measured again, at most twice, and the surviving
# pairs are pooled across attempts; the B values behind B/A0 come from
# the same adopted pairs, never from a discarded run. If four pairs
# still do not survive, the script reports that the measurement could
# not be completed and exits non-zero. The result is, per scenario, the
# median of the per-pair B/A ratios (the average of the middle two),
# the B/A0 ratio, and the A/A pair ratios that show the noise width.
# Speed is reported only; it never stops the work.
set -euo pipefail

A_DIR=$1
B_DIR=$2
A0_DIR=${3:-}

BENCH_REL=crates/fraktor-actor/tests/actor_system_send_stage1_bench.rs
for dir in "$A_DIR" "$A0_DIR"; do
  if [ -n "$dir" ] && [ ! -f "$dir/$BENCH_REL" ]; then
    cp "$B_DIR/$BENCH_REL" "$dir/$BENCH_REL"
    echo "copied the bench test into $dir (the comparison commits predate it)"
  fi
done

run_bench() {
  local dir=$1 out=$2
  # The harness may glue the first printed line onto the test name, so
  # the BENCH records are cut out of the lines instead of anchored.
  if ! (cd "$dir" && cargo test --release -p fraktor-actor --test actor_system_send_stage1_bench \
    -- --ignored --nocapture 2>"$out.err" | grep -o 'BENCH [A-Z0-9]* [0-9.]*' > "$out") || [ ! -s "$out" ]; then
    echo "bench run failed in $dir; cargo said:" >&2
    cat "$out.err" >&2
    return 1
  fi
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

labels=(A B B A A B B A)
dirs=("$A_DIR" "$B_DIR" "$B_DIR" "$A_DIR" "$A_DIR" "$B_DIR" "$B_DIR" "$A_DIR")
attempt=0
while [ "$attempt" -lt 3 ]; do
  attempt=$((attempt + 1))
  echo "attempt $attempt, measured at $(date '+%Y-%m-%d %H:%M %Z'), load: $(uptime | sed 's/.*load averages*: //')"
  for i in 0 1 2 3 4 5 6 7; do
    run_bench "${dirs[$i]}" "$work/a${attempt}_run$i.txt"
    echo "run $((i + 1)) (${labels[$i]}): $(tr '\n' ' ' < "$work/a${attempt}_run$i.txt")"
  done
  if [ -n "$A0_DIR" ]; then
    run_bench "$A0_DIR" "$work/a${attempt}_a0a.txt"
    run_bench "$A0_DIR" "$work/a${attempt}_a0b.txt"
    echo "run A0 (1): $(tr '\n' ' ' < "$work/a${attempt}_a0a.txt")"
    echo "run A0 (2): $(tr '\n' ' ' < "$work/a${attempt}_a0b.txt")"
  fi
  # Stop early once four pairs have survived; the python step re-checks
  # the same condition and decides whether the measurement is complete.
  if python3 - "$work" "$attempt" <<'PY'
import sys
from pathlib import Path

work, attempts = Path(sys.argv[1]), int(sys.argv[2])

def parse(path):
    values = {}
    for line in path.read_text().splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[0] == "BENCH":
            values[parts[1]] = float(parts[2])
    return values

labels = ["A", "B", "B", "A", "A", "B", "B", "A"]
pairs = [(0, 1), (2, 3), (4, 5), (6, 7)]
valid = 0
for a in range(1, attempts + 1):
    runs = [parse(work / f"a{a}_run{i}.txt") for i in range(8)]
    if any(set(r) != {"CONTROL", "S1", "S2", "S3", "S4"} for r in runs):
        continue
    for i, j in pairs:
        a_idx, b_idx = (i, j) if labels[i] == "A" else (j, i)
        ca, cb = runs[a_idx]["CONTROL"], runs[b_idx]["CONTROL"]
        if abs(ca - cb) / min(ca, cb) <= 0.05:
            valid += 1
sys.exit(0 if valid >= 4 else 1)
PY
  then
    break
  fi
done

python3 - "$work" "$attempt" <<'PY'
import statistics, sys
from pathlib import Path

work, attempts = Path(sys.argv[1]), int(sys.argv[2])

def parse(path):
    values = {}
    for line in path.read_text().splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[0] == "BENCH":
            values[parts[1]] = float(parts[2])
    missing = {"CONTROL", "S1", "S2", "S3", "S4"} - set(values)
    if missing:
        raise SystemExit(f"bench output in {path.name} lacks {sorted(missing)}; the run is not usable")
    return values

labels = ["A", "B", "B", "A", "A", "B", "B", "A"]
pairs = [(0, 1), (2, 3), (4, 5), (6, 7)]
adopted = []  # (a_run, b_run) in measurement order, first four survivors
a0_runs = []
for a in range(1, attempts + 1):
    runs = [parse(work / f"a{a}_run{i}.txt") for i in range(8)]
    survivors = []
    for i, j in pairs:
        a_idx, b_idx = (i, j) if labels[i] == "A" else (j, i)
        ca, cb = runs[a_idx]["CONTROL"], runs[b_idx]["CONTROL"]
        if abs(ca - cb) / min(ca, cb) <= 0.05:
            survivors.append((runs[a_idx], runs[b_idx]))
    if survivors and (work / f"a{a}_a0a.txt").exists():
        a0_runs += [parse(work / f"a{a}_a0a.txt"), parse(work / f"a{a}_a0b.txt")]
    adopted += survivors
adopted = adopted[:4]

if len(adopted) < 4:
    print(f"measurement incomplete: only {len(adopted)} of 4 pairs survived "
          f"the control check after {attempts} attempt(s); "
          "the stage's re-measurement cap is exhausted, so no ratio is reported")
    sys.exit(1)

a0 = {k: statistics.median([r[k] for r in a0_runs]) for k in a0_runs[0]} if a0_runs else None
print(f"{'scenario':<9} {'B/A':>7} {'B/A0':>7}  A/A pair ratios")
for s in ["S1", "S2", "S3", "S4"]:
    ratios = [b[s] / a[s] for a, b in adopted]
    aa = [adopted[k + 1][0][s] / adopted[k][0][s] for k in range(len(adopted) - 1)]
    b_med = statistics.median([b[s] for _, b in adopted])
    b_a0 = f"{b_med / a0[s]:.3f}" if a0 else "-"
    aa_text = ", ".join(f"{v:.3f}" for v in aa)
    print(f"{s:<9} {statistics.median(ratios):>7.3f} {b_a0:>7}  {aa_text}")
PY
