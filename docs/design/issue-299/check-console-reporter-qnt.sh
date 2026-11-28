#!/usr/bin/env bash
# Reproduces the checks of crates/fraktor-testkit/specs/console_reporter.qnt
# that docs/design/issue-183-metrics-kit.md §6 reports:
#
#   (0) typecheck, every run as a test, and `invariant` over 10000 traces;
#   (a) one mutant per invariant and per guard of the contract, each a single
#       edit, each violating the invariant it targets with a fixed seed while
#       the unmutated spec does not;
#   (a') one mutant per guard that no state invariant can see, each a single
#       edit, each failing the run named for it while the unmutated spec
#       passes that run;
#   (b) how many of 1000 traces take each action.
#
# Needs: bash, npm (run `npm ci` first; quint is pinned in package-lock.json),
# python3 (writes the mutants) and jq (counts the actions). Run from the
# repository root. The mutants are written to a temporary directory, never
# under crates/*/specs, which scripts/quint-check-specs.sh checks.
set -euo pipefail

quint=./node_modules/.bin/quint
spec=crates/fraktor-testkit/specs/console_reporter.qnt
seed=0x2a
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

[ -x "$quint" ] || { echo "run npm ci first: $quint is missing" >&2; exit 1; }
for tool in python3 jq; do
  command -v "$tool" >/dev/null || { echo "$tool is required but not found on PATH" >&2; exit 1; }
done
echo "quint $("$quint" --version)"

echo "== (0) typecheck, test, invariant"
"$quint" typecheck "$spec"
"$quint" test "$spec" | grep -E 'passing|failed'
"$quint" run --invariant=invariant --max-steps=20 --max-samples=10000 --seed="$seed" "$spec" \
  | grep -E '^\[ok\]|violation'

# Each mutant: name, the invariant it must violate, the text to replace (it
# must occur exactly once), and the replacement. Fields are separated by ~;
# \n in the texts is a newline.
mutants=(
  "construct starts the thread without the interval~threadOnlyWithConsoleAndInterval~thread' = if (console and positiveInterval) Waiting~thread' = if (console) Waiting"
  "lookAtTheThread goes back to Idle on Joined~flagOnlyAfterAStopBegins~      | Joined => Returned~      | Joined => Idle"
  "wakeToStop without the flag~leavesOnlyOnTheFlag~    stopFlag,\n    thread' = Finished,~    thread' = Finished,"
  "setTheFlag keeps the stop lock~stopLockOnlyAroundTheFlag~    stopFlag' = true,\n    stopLock' = Free,~    stopFlag' = true,\n    stopLock' = stopLock,"
  "joinReturn before the thread leaves~noReportOnceACallReturned~    calls.get(c) == Joiner,\n    thread == Finished,~    calls.get(c) == Joiner,"
  "lookAtTheThread leaves the handle Running~oneJoinerAtATime~handle' = if (handle == Running) Joining else handle,~handle' = handle,"
  "joinReturn puts the handle back to Running~joinedAtMostOnce~    handle' = Joined,\n    joins' = joins + 1,~    handle' = Running,\n    joins' = joins + 1,"
  "beginDrop while a shutdown or report runs~nothingRunsAfterTheDrop~    Shutdowns.forall(c => isFree(c)),\n~"
  "beginShutdown without the contract~noDeadlock~    contract implies writerLock != HeldBy(c),\n    takeTheStopLock(c, false),~    takeTheStopLock(c, false),"
  "beginDrop without the contract~noDeadlock~    contract implies writerLock != HeldBy(Drop),\n~"
  "beginReport without the contract~noDeadlock~    contract implies writerLock != HeldBy(c),\n    printing' = printing.set(c, WantsTheOutput),~    printing' = printing.set(c, WantsTheOutput),"
  "holdTheWriter inside a stop or report~noStopOrReportHoldsTheWriter~    writerLock == Free,\n    isFree(c),\n    writerHolds < 2,~    writerLock == Free,\n    writerHolds < 2,"
  "finishPrinting keeps the output Mutex~theOutputIsHeldOnlyToPrint~    canWrite(x),\n    outputMutex' = Free,~    canWrite(x),\n    outputMutex' = outputMutex,"
)

# Each guard mutant: name, the run that must fail on it, the text to replace
# (exactly once), and the replacement. Every state such a mutant reaches is
# also reachable in the unmutated spec with two steps swapped, so no state
# invariant sees it; only a run does.
guard_mutants=(
  "wakeToStop without the stop lock~theThreadCannotWakeToStopWhileTheStopLockIsHeldTest~    stopLock == Free,\n    stopFlag,\n    thread' = Finished,~    stopFlag,\n    thread' = Finished,"
  "wakeToReport without the stop lock~theThreadCannotWakeToReportWhileTheStopLockIsHeldTest~    stopLock == Free,\n    not(stopFlag),\n    reports < 3,~    not(stopFlag),\n    reports < 3,"
)

# mutate <mutant> <old> <new>: writes the spec with the one edit to <mutant>.
mutate() {
  local mutant=$1 old=$2 new=$3
  mkdir -p "$(dirname "$mutant")"
  python3 -I - "$spec" "$mutant" "$old" "$new" <<'PY'
import sys
spec, mutant, old, new = sys.argv[1:]
old = old.replace("\\n", "\n")
new = new.replace("\\n", "\n")
text = open(spec, encoding="utf-8").read()
count = text.count(old)
if count != 1:
    sys.exit(f"the text to replace occurs {count} times, not once: {old!r}")
open(mutant, "w", encoding="utf-8").write(text.replace(old, new))
PY
}

echo "== (a) mutants (seed $seed, 20 steps, 10000 traces)"
failed=0
n=0
for row in "${mutants[@]}"; do
  n=$((n + 1))
  IFS='~' read -r name invariant old new <<<"$row"
  mutant=$work/mutant$n/console_reporter.qnt
  mutate "$mutant" "$old" "$new"
  if cmp -s "$spec" "$mutant"; then
    echo "FAIL $name: the edit changed nothing"; failed=1; continue
  fi
  if ! "$quint" typecheck "$mutant" >/dev/null 2>&1; then
    echo "FAIL $name: the mutant does not typecheck"; failed=1; continue
  fi
  if ! "$quint" run --invariant="$invariant" --max-steps=20 --max-samples=10000 --seed="$seed" "$spec" \
    >"$work/base$n.out" 2>&1; then
    echo "FAIL $name: the unmutated spec violates $invariant"; failed=1; continue
  fi
  if "$quint" run --invariant="$invariant" --max-steps=20 --max-samples=10000 --seed="$seed" "$mutant" \
    >"$work/mutant$n.out" 2>&1; then
    echo "FAIL $name: no violation of $invariant"; failed=1; continue
  fi
  if ! grep -q '\[violation\]' "$work/mutant$n.out"; then
    echo "FAIL $name: quint failed without a violation"; cat "$work/mutant$n.out"; failed=1; continue
  fi
  echo "ok   $name: violates $invariant"
done

echo "== (a') guard mutants, each caught by a run"
for row in "${guard_mutants[@]}"; do
  n=$((n + 1))
  IFS='~' read -r name run old new <<<"$row"
  mutant=$work/mutant$n/console_reporter.qnt
  mutate "$mutant" "$old" "$new"
  if cmp -s "$spec" "$mutant"; then
    echo "FAIL $name: the edit changed nothing"; failed=1; continue
  fi
  if ! "$quint" typecheck "$mutant" >/dev/null 2>&1; then
    echo "FAIL $name: the mutant does not typecheck"; failed=1; continue
  fi
  if ! "$quint" test --match="^$run\$" "$spec" >"$work/base$n.out" 2>&1 \
    || ! grep -q ' 1 passing' "$work/base$n.out"; then
    echo "FAIL $name: the unmutated spec does not pass $run"; cat "$work/base$n.out"; failed=1; continue
  fi
  if "$quint" test --match="^$run\$" "$mutant" >"$work/mutant$n.out" 2>&1; then
    echo "FAIL $name: $run still passes"; failed=1; continue
  fi
  if ! grep -q "$run failed" "$work/mutant$n.out"; then
    echo "FAIL $name: quint failed without $run failing"; cat "$work/mutant$n.out"; failed=1; continue
  fi
  echo "ok   $name: fails $run"
done

echo "== (b) actions taken in 1000 traces (seed 0x1, 20 steps)"
mkdir -p "$work/itf"
"$quint" run --mbt --max-steps=20 --max-samples=1000 --n-traces=1000 --seed=0x1 \
  --out-itf="$work/itf/t{seq}.itf.json" "$spec" >/dev/null
actions=(construct wakeToReport takeTheOutput finishPrinting wakeToStop beginShutdown beginReport
  beginDrop setTheFlag lookAtTheThread joinReturn waitReturn holdTheWriter releaseTheWriter)
counts=$(jq -r '[.states[]["mbt::actionTaken"]] | unique | .[]' "$work"/itf/*.itf.json | sort | uniq -c)
for action in "${actions[@]}"; do
  c=$(awk -v a="$action" '$2 == a { print $1 }' <<<"$counts")
  echo "${c:-0} $action"
  [ -n "$c" ] || { echo "FAIL $action is never taken"; failed=1; }
done

[ "$failed" -eq 0 ] && echo "all checks passed" || { echo "some checks failed" >&2; exit 1; }
