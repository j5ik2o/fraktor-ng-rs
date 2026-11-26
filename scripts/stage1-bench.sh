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
# A pair whose control loop values drift by more than 5 percent is
# discarded as machine noise and reported. The result is, per scenario,
# the median of the per-pair B/A ratios (the average of the middle two),
# the B/A0 ratio against the median A0 run, and the A/A pair ratios that
# show the noise width. Speed is reported only; it never stops the work.
set -euo pipefail

A_DIR=$1
B_DIR=$2
A0_DIR=${3:-}

run_bench() {
  local dir=$1 out=$2
  (cd "$dir" && cargo test --release -p fraktor-actor --test actor_system_send_stage1_bench \
    -- --ignored --nocapture 2>/dev/null | grep '^BENCH ' > "$out")
  test -s "$out"
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

echo "measured at $(date '+%Y-%m-%d %H:%M %Z'), load: $(uptime | sed 's/.*load averages*: //')"
labels=(A B B A A B B A)
dirs=("$A_DIR" "$B_DIR" "$B_DIR" "$A_DIR" "$A_DIR" "$B_DIR" "$B_DIR" "$A_DIR")
for i in 0 1 2 3 4 5 6 7; do
  run_bench "${dirs[$i]}" "$work/run$i.txt"
  echo "run $((i + 1)) (${labels[$i]}): $(tr '\n' ' ' < "$work/run$i.txt")"
done
if [ -n "$A0_DIR" ]; then
  run_bench "$A0_DIR" "$work/a0a.txt"
  run_bench "$A0_DIR" "$work/a0b.txt"
  echo "run A0 (1): $(tr '\n' ' ' < "$work/a0a.txt")"
  echo "run A0 (2): $(tr '\n' ' ' < "$work/a0b.txt")"
fi

python3 - "$work" <<'PY'
import statistics, sys
from pathlib import Path

work = Path(sys.argv[1])

def parse(name):
    values = {}
    for line in (work / name).read_text().splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[0] == "BENCH":
            values[parts[1]] = float(parts[2])
    return values

runs = [parse(f"run{i}.txt") for i in range(8)]
labels = ["A", "B", "B", "A", "A", "B", "B", "A"]
pairs = [(0, 1), (2, 3), (4, 5), (6, 7)]
a0 = None
if (work / "a0a.txt").exists():
    a_runs = [parse("a0a.txt"), parse("a0b.txt")]
    a0 = {k: statistics.median([r[k] for r in a_runs]) for k in a_runs[0]}

scenarios = ["S1", "S2", "S3", "S4"]
print(f"{'scenario':<9} {'B/A':>7} {'B/A0':>7}  A/A pair ratios   discarded pairs")
for s in scenarios:
    ratios, aa, discarded = [], [], 0
    for i, j in pairs:
        a_idx, b_idx = (i, j) if labels[i] == "A" else (j, i)
        ca, cb = runs[a_idx]["CONTROL"], runs[b_idx]["CONTROL"]
        if abs(ca - cb) / min(ca, cb) > 0.05:
            discarded += 1
            continue
        ratios.append(runs[b_idx][s] / runs[a_idx][s])
    a_vals = [runs[i][s] for i in range(8) if labels[i] == "A"]
    aa = [a_vals[1] / a_vals[0], a_vals[3] / a_vals[2]]
    med = statistics.median(ratios) if ratios else float("nan")
    b_med = statistics.median([runs[i][s] for i in range(8) if labels[i] == "B"])
    b_a0 = f"{b_med / a0[s]:.3f}" if a0 else "-"
    aa_text = ", ".join(f"{v:.3f}" for v in aa)
    print(f"{s:<9} {med:>7.3f} {b_a0:>7}  {aa_text:<17} {discarded}")
PY
