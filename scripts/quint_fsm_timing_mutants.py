#!/usr/bin/env python3
"""Rechecks crates/fraktor-actor/specs/fsm_timing.qnt the way #341-#344 ask.

Run from the repository root after `npm ci` (it uses the quint pinned in
package-lock.json):

    python3 scripts/quint_fsm_timing_mutants.py            # (a) and (b)
    python3 scripts/quint_fsm_timing_mutants.py --full     # every invariant on every mutant
    python3 scripts/quint_fsm_timing_mutants.py --only M05,G1M05

What it checks (design docs/design/issue-198-fsm-timing-actor-ref.md, section 6):

- baseline: the spec typechecks, every run passes, and `invariant` holds in a
  fixed-seed simulation.
- (a) every mutant below is a one-place text change of a real action. Each is
  applied to a copy in a temporary directory (nothing is written under
  crates/*/specs/, which CI globs), and must be killed by every checker it
  names: an invariant that a fixed-seed `quint run` violates, or a run that
  `quint test` fails. A mutant marked equivalent must not violate an
  invariant or fail a positive run; failing a negative control (a run named
  bug*Test) is reported, since it means the mutant hides that defect.
  Every invariant of the spec must be the expected killer of some mutant.
- (b) witnesses: every action and branch of `step` is taken in some trace.

The exit status is non-zero when any of this does not hold, and also when a
quint process ends in a way that gives no result (a crash, a kill, a `quint
test` report that does not name every run). quint runs with colour turned off
(FORCE_COLOR is dropped and NO_COLOR set), and its output is read with any
escape sequences removed. The simulation uses quint's default (Rust) backend
with a fixed seed, which repeats exactly.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

SPEC = Path("crates/fraktor-actor/specs/fsm_timing.qnt")
QUINT = Path("node_modules/.bin/quint")
SEED = "0x1"
LOAD_LIMIT = 50.0

INVARIANTS = [
    "armedMatchesSelection",
    "infiniteNeverArms",
    "staleStateNeverFires",
    "currentGenStateMarkIsLive",
    "staleNamedNeverFires",
    "stoppedNeverArms",
    "stoppedNeverFires",
    "stateTimerFiresOnce",
    "singleLeavesTable",
    "singleFiresOnce",
    "repeatStaysInTable",
]


@dataclass
class Mutant:
    ident: str
    summary: str
    # (anchor, old, new): `old` must occur exactly once inside the definition
    # named `anchor`, and nowhere is anything else changed.
    edits: list[tuple[str, str, str]]
    # Invariant names or run names (ending in Test) that must kill it.
    killers: set[str] = field(default_factory=set)
    equivalent: bool = False


MUTANTS = [
    Mutant("M01", "NoOverride never arms the default", [
        ("selectionShouldArm", "| NoOverride => defaultHasTimeout(targetState)", "| NoOverride => false")],
        {"armedMatchesSelection", "defaultTimeoutOnEnterAndStayTest"}),
    Mutant("M02", "forMax(Infinite) arms", [
        ("selectionShouldArm", "| OverrideInfinite => false", "| OverrideInfinite => true")],
        {"armedMatchesSelection", "infiniteNeverArms", "forMaxInfiniteDisablesDefaultTimeoutTest"}),
    Mutant("M03", "forMax(After) does not arm", [
        ("selectionShouldArm", "| OverrideAfter => true", "| OverrideAfter => false")],
        {"armedMatchesSelection", "forMaxAfterOverridesNoDefaultTest"}),
    Mutant("M04", "an ordinary message does not advance stateGen", [
        ("receiveOrdinary", "receiveOrdinaryWith(stateGen + 1, kind, target, ov)",
         "receiveOrdinaryWith(stateGen, kind, target, ov)")],
        {"staleStateNeverFires", "currentGenStateMarkIsLive", "defaultTimeoutOnEnterAndStayTest"}),
    Mutant("M05", "an accepted named mark does not advance stateGen (#341, case 8)", [
        ("receiveNamedMark", "acceptNamedWith(stateGen + 1, m, kind, target, ov)",
         "acceptNamedWith(stateGen, m, kind, target, ov)")],
        {"currentGenStateMarkIsLive", "namedMarkRearmDropsOldStateMarkTest",
         "namedTimerMarkCancelsStateTimerTest"}),
    Mutant("G1", "the production guard on the slot (fsm.rs:965, stActive for state_timer_armed), alone; "
           "hides bugNamedMarkKeepsGenerationTest", [
        ("receiveStateMark", "if (m.inst == curInst and m.gen == stateGen)",
         "if (m.inst == curInst and stActive and m.gen == stateGen)")],
        equivalent=True),
    Mutant("G1M05", "the production guard with M05: the guard drops the stale mark, and both case 8 runs "
           "still fail through their invariant expectations", [
        ("receiveStateMark", "if (m.inst == curInst and m.gen == stateGen)",
         "if (m.inst == curInst and stActive and m.gen == stateGen)"),
        ("receiveNamedMark", "acceptNamedWith(stateGen + 1, m, kind, target, ov)",
         "acceptNamedWith(stateGen, m, kind, target, ov)")],
        {"currentGenStateMarkIsLive", "namedMarkRearmDropsOldStateMarkTest",
         "namedTimerMarkCancelsStateTimerTest"}),
    Mutant("M06", "a state mark of an earlier instance is accepted", [
        ("receiveStateMark", "if (m.inst == curInst and m.gen == stateGen)", "if (m.gen == stateGen)")],
        {"staleStateNeverFires", "restartInstanceIdDropsOldMarksTest"}),
    Mutant("M07", "a named mark of an older generation is accepted", [
        ("namedMarkMatches", " and table.get(m.name).gen == m.gen", "")],
        {"staleNamedNeverFires", "cancelledTimerQueuedMarkDroppedTest"}),
    Mutant("M08", "a named mark of an earlier instance is accepted", [
        ("namedMarkMatches", "m.inst == inst and ", "")],
        {"staleNamedNeverFires", "restartInstanceIdDropsOldMarksTest"}),
    Mutant("M09", "cancel leaves the timer in the table", [
        ("cancelNamedTimer", "timers' = without(timers, name),", "timers' = timers,")],
        {"staleNamedNeverFires", "repeatedTimerKeepsFiringUntilCancelledTest"}),
    Mutant("M10", "replacing a timer keeps the old generation", [
        ("startNamedTimer", "gen: namedGen + 1,",
         "gen: if (timers.keys().contains(name)) timers.get(name).gen else namedGen + 1,")],
        {"staleNamedNeverFires"}),
    Mutant("M11", "stop leaves the named timers", [
        ("stopActor", "timers' = Map(),", "timers' = timers,")],
        {"stoppedNeverArms", "stoppedNeverFires", "stopClearsAllTimersTest"}),
    Mutant("M12", "stop leaves the executor's state timer row", [
        ("stopActor", "liveSid' = -1,", "liveSid' = liveSid,")],
        {"stoppedNeverArms", "stoppedNeverFires", "stopClearsAllTimersTest"}),
    Mutant("M13", "stop leaves the slot active", [
        ("stopActor", "stActive' = false,", "stActive' = stActive,")],
        {"stoppedNeverArms", "stopClearsAllTimersTest"}),
    Mutant("M14", "an accepted Single stays in the table", [
        ("acceptNamedWith", "else if (single) without(timers, m.name) else timers,", "else timers,")],
        {"singleLeavesTable", "singleFiresOnce", "staleNamedNeverFires",
         "singleRestartedUnderSameNameFiresOnceTest"}),
    Mutant("M15", "a repeating timer leaves the table at its first mark (#343)", [
        ("acceptNamedWith", "without(timers, m.name) else timers,",
         "without(timers, m.name) else without(timers, m.name),")],
        {"repeatStaysInTable", "repeatedTimerKeepsFiringUntilCancelledTest"}),
    Mutant("M16", "goto into a nonexistent state keeps the named timers", [
        ("receiveOrdinaryWith", "timers' = if (gotoNonExistent(kind, target)) Map() else timers,",
         "timers' = timers,")],
        {"staleNamedNeverFires", "gotoNonExistentClearsTimersTest"}),
    Mutant("M17", "arming registers no executor deadline", [
        ("applyTransitionWith", "liveSid' = if (arms) keptNextSid else -1,", "liveSid' = -1,")],
        {"stateTimerFiresOnceTest", "stateTimeoutRearmsWhenTargetHasDefaultTest"}),
    Mutant("M19", "a state timeout mark never keeps the slot", [
        ("applyTransitionWith", "(if (keepsSlot) keptActive else false)", "false")],
        {"armedMatchesSelection", "forMaxAfterOverridesNoDefaultTest"}),
    Mutant("M20", "restart keeps the instance id", [
        ("restartActor", "restartWith(nextInst, nextInst + 1)", "restartWith(curInst, nextInst)")],
        {"staleNamedNeverFires", "restartInstanceIdDropsOldMarksTest"}),
    Mutant("M21", "the executor fires one arming twice (#342)", [
        ("stateTimerDue", "stateTimerDueWith(true)", "stateTimerDueWith(false)")],
        {"stateTimerFiresOnce", "currentGenStateMarkIsLive", "staleStateNeverFires", "stateTimerFiresOnceTest"}),
    Mutant("M22", "restart keeps the named timers", [
        ("restartWith", "timers' = Map(),", "timers' = timers,")],
        {"staleNamedNeverFires"}),
    Mutant("M24", "a dropped named mark stays in the mailbox", [
        ("dropNamed", "namedMarks' = dropFirst(namedMarks, m),", "namedMarks' = namedMarks,")],
        {"gotoNonExistentClearsTimersTest", "cancelledTimerQueuedMarkDroppedTest"}),
    Mutant("M25", "a state timeout into a nonexistent state clears the slot (the earlier model, #344)", [
        ("applyTransitionWith", "val keepsSlot = fromStateTimeout and not(valid and needsDeadline)",
         "val keepsSlot = fromStateTimeout and valid and not(needsDeadline)")],
        {"stateTimeoutToNonExistentKeepsSlotTest"}),
    Mutant("M26", "a dropped state mark stays in the mailbox", [
        ("dropState", "stateMarks' = dropFirst(stateMarks, m),", "stateMarks' = stateMarks,")],
        {"namedMarkRearmDropsOldStateMarkTest"}),
    Mutant("M27", "an accepted state mark stays in the mailbox", [
        ("acceptState", "stateMarks' = dropFirst(stateMarks, m),", "stateMarks' = stateMarks,")],
        {"currentGenStateMarkIsLive", "staleStateNeverFires", "forMaxAfterOverridesNoDefaultTest"}),
    Mutant("M28", "a fresh arming is born already fired", [
        ("applyTransitionWith", "stIssued' = if (arms) false else keptIssued,", "stIssued' = keptIssued,")],
        {"stateTimeoutRearmsWhenTargetHasDefaultTest"}),
    Mutant("M29", "a goto into a nonexistent state arms under forMax(After)", [
        ("applyTransitionWith", "val arms = valid and actualArm", "val arms = actualArm")],
        {"armedMatchesSelection", "gotoNonExistentUnderForMaxAfterArmsNothingTest"}),
]

# (b): one predicate per action and branch of `step`.
WITNESSES = {
    "wOrdinaryArmsAfter": 'lastAction == "ordinary" and lastOverride == OverrideAfter and liveSid != -1',
    "wOrdinaryInfinite": 'lastAction == "ordinary" and lastOverride == OverrideInfinite',
    "wOrdinaryArmsDefault": 'lastAction == "ordinary" and lastOverride == NoOverride and liveSid != -1',
    "wOrdinaryArmsNothing": 'lastAction == "ordinary" and ValidStates.contains(lastTarget) and liveSid == -1',
    "wOrdinaryNonexistent": 'lastAction == "ordinary" and not(ValidStates.contains(lastTarget))',
    "wStartNew": 'lastAction == "start:new"',
    "wStartReplace": 'lastAction == "start:replace"',
    "wCancelPresent": 'lastAction == "cancel:present"',
    "wCancelAbsent": 'lastAction == "cancel:absent"',
    "wNamedDue": 'lastAction == "namedDue"',
    "wTwoNamedMarksQueued": "namedMarks.length() >= 2",
    "wStateDue": 'lastAction == "stateDue"',
    "wAcceptNamedSingle": 'lastAction == "acceptNamed" and acceptedRepeat == NoName and ValidStates.contains(lastTarget)',
    "wAcceptNamedRepeat": 'lastAction == "acceptNamed" and acceptedRepeat != NoName',
    "wAcceptNamedNonexistent": 'lastAction == "acceptNamed" and not(ValidStates.contains(lastTarget))',
    "wDropNamedInstance": 'lastAction == "dropNamed:inst"',
    "wDropNamedAbsent": 'lastAction == "dropNamed:absent"',
    "wDropNamedGeneration": 'lastAction == "dropNamed:gen"',
    "wAcceptStateKeep": 'lastAction == "acceptState" and keptAfterMark and ValidStates.contains(lastTarget)',
    "wAcceptStateRearm": 'lastAction == "acceptState" and liveSid != -1',
    "wAcceptStateNonexistent": 'lastAction == "acceptState" and not(ValidStates.contains(lastTarget))',
    "wDropStateInstance": 'lastAction == "dropState:inst"',
    "wDropStateGeneration": 'lastAction == "dropState:gen"',
    "wRestart": 'lastAction == "restart"',
    "wStop": 'lastAction == "stop"',
    "wOldMarksAfterRestart": (
        "curInst > 0 and (stateMarks.indices().exists(i => stateMarks[i].inst < curInst)"
        " or namedMarks.indices().exists(i => namedMarks[i].inst < curInst))"),
    # #341: a named acceptance while a state mark of this instance that was
    # current just before it is queued, once with a new arming and once
    # without. The acceptance advances stateGen by one, so such a mark has
    # gen == stateGen - 1; a mark that was already older does not count.
    "wNamedAcceptOverQueuedStateMarkRearm": (
        'lastAction == "acceptNamed" and liveSid != -1'
        " and stateMarks.indices().exists(i => stateMarks[i].inst == curInst and stateMarks[i].gen == stateGen - 1)"),
    "wNamedAcceptOverQueuedStateMarkNoArm": (
        'lastAction == "acceptNamed" and liveSid == -1'
        " and stateMarks.indices().exists(i => stateMarks[i].inst == curInst and stateMarks[i].gen == stateGen - 1)"),
}

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
# Colour codes break the patterns below, so quint never gets FORCE_COLOR.
QUINT_ENV = {k: v for k, v in os.environ.items() if k != "FORCE_COLOR"} | {"NO_COLOR": "1"}

TOP_LEVEL = re.compile(r"^  (?:action|val|pure def|pure val|def|run|var|type) (\w+)", re.M)


def definition_span(text: str, name: str) -> tuple[int, int]:
    starts = [(m.start(), m.group(1)) for m in TOP_LEVEL.finditer(text)]
    for index, (start, found) in enumerate(starts):
        if found == name:
            end = starts[index + 1][0] if index + 1 < len(starts) else len(text)
            return start, end
    raise SystemExit(f"no top-level definition named {name}")


def apply_edits(text: str, edits: list[tuple[str, str, str]]) -> str:
    for anchor, old, new in edits:
        start, end = definition_span(text, anchor)
        body = text[start:end]
        if body.count(old) != 1:
            raise SystemExit(f"{anchor}: expected exactly one {old!r}, found {body.count(old)}")
        text = text[:start] + body.replace(old, new) + text[end:]
    return text


def wait_for_load() -> None:
    while os.getloadavg()[0] > LOAD_LIMIT:
        time.sleep(10)


def quint(*args: str) -> subprocess.CompletedProcess[str]:
    wait_for_load()
    out = subprocess.run([str(QUINT), *args], capture_output=True, text=True, env=QUINT_ENV)
    out.stdout = ANSI.sub("", out.stdout)
    out.stderr = ANSI.sub("", out.stderr)
    return out


def run_names(text: str) -> list[str]:
    return re.findall(r"^  run (\w+)", text, re.M)


def failing_runs(path: Path, runs: list[str]) -> set[str]:
    out = quint("test", str(path))
    failed = set(re.findall(r"^\s+\d+\) (\w+) failed", out.stdout, re.M))
    passed = set(re.findall(r"^\s+ok (\w+) passed", out.stdout, re.M))
    # `quint test` exits 0 when every run passes and 1 when some fail. Any
    # other exit (a crash, a kill), or a report that does not name each run
    # exactly once as passed or failed, is no result: an empty set read from
    # it would let the baseline and the equivalent mutant pass unchecked.
    if (out.returncode not in (0, 1) or (out.returncode == 1) != bool(failed)
            or passed & failed or passed | failed != set(runs)):
        raise SystemExit(f"quint test gave no result on {path} (exit {out.returncode}):\n{out.stdout}\n{out.stderr}")
    return failed


def violates(path: Path, invariant: str, steps: int, samples: int) -> bool:
    out = quint("run", f"--invariant={invariant}", f"--max-steps={steps}",
                f"--max-samples={samples}", f"--seed={SEED}", "--verbosity=1", str(path))
    if "[violation]" in out.stdout or "[violation]" in out.stderr:
        return True
    if out.returncode != 0 or "[ok]" not in out.stdout + out.stderr:
        raise SystemExit(f"quint run failed on {path} ({invariant}):\n{out.stdout}\n{out.stderr}")
    return False


def check_mutant(mutant: Mutant, base: str, runs: list[str], work: Path, steps: int, samples: int,
                 full: bool) -> tuple[Mutant, set[str], list[str]]:
    path = work / f"{mutant.ident}.qnt"
    path.write_text(apply_edits(base, mutant.edits))
    typed = quint("typecheck", str(path))
    if typed.returncode != 0:
        return mutant, set(), [f"does not typecheck:\n{typed.stdout}{typed.stderr}"]
    killed = failing_runs(path, runs)
    # An equivalent mutant is simulated once against the conjunction
    # `invariant`; --full checks every invariant separately.
    if full:
        wanted = INVARIANTS
    elif mutant.equivalent:
        wanted = ["invariant"]
    else:
        wanted = sorted(k for k in mutant.killers if k in INVARIANTS)
    for invariant in wanted:
        if violates(path, invariant, steps, samples):
            killed.add(invariant)
    problems = []
    # A negative control (a run named bug*Test) shows a defect; a mutant that
    # makes one fail hides that defect, which is reported but is not a change
    # of the model's behaviour.
    behaviour = {k for k in killed if not k.startswith("bug")}
    if mutant.equivalent and behaviour:
        problems.append(f"marked equivalent but killed by {sorted(behaviour)}")
    missing = mutant.killers - killed
    if missing:
        problems.append(f"not killed by {sorted(missing)}")
    return mutant, killed, problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--full", action="store_true", help="check every invariant on every mutant")
    parser.add_argument("--only", help="comma-separated mutant ids")
    parser.add_argument("--jobs", type=int, default=2, help="parallel quint processes (at most 4)")
    parser.add_argument("--steps", type=int, default=20)
    parser.add_argument("--samples", type=int, default=10000)
    parser.add_argument("--no-witnesses", action="store_true")
    args = parser.parse_args()
    jobs = max(1, min(args.jobs, 4))

    if not QUINT.exists():
        print(f"{QUINT} is missing: run `npm ci` first", file=sys.stderr)
        return 2
    base = SPEC.read_text()
    failures: list[str] = []

    work = Path(tempfile.mkdtemp(prefix="fsm_timing_mutants_"))
    try:
        # Baseline.
        baseline = work / "baseline.qnt"
        shutil.copy(SPEC, baseline)
        if quint("typecheck", str(baseline)).returncode != 0:
            print("baseline does not typecheck", file=sys.stderr)
            return 1
        runs = run_names(base)
        broken = failing_runs(baseline, runs)
        print(f"baseline: {len(runs)} runs, failing: {sorted(broken) or 'none'}", flush=True)
        if broken:
            failures.append(f"baseline runs fail: {sorted(broken)}")
        if violates(baseline, "invariant", args.steps, args.samples):
            failures.append("baseline violates invariant")
        print(f"baseline invariant ({args.steps} steps x {args.samples} traces, seed {SEED}): "
              f"{'violated' if 'baseline violates invariant' in failures else 'holds'}")

        # (a) Mutants.
        selected = MUTANTS
        if args.only:
            wanted = set(args.only.split(","))
            selected = [m for m in MUTANTS if m.ident in wanted]
        for mutant in selected:
            unknown = {k for k in mutant.killers if k not in INVARIANTS and k not in runs}
            if unknown:
                failures.append(f"{mutant.ident}: unknown killers {sorted(unknown)}")
        if not args.only:
            uncovered = set(INVARIANTS) - set().union(*(m.killers for m in MUTANTS))
            if uncovered:
                failures.append(f"invariants no mutant expects to kill: {sorted(uncovered)}")
        print()
        print(f"mutants: {len(selected)} of {len(MUTANTS)}")
        print("| mutant | change | expected killers | killed by | result |")
        print("|:---|:---|:---|:---|:---|")
        with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
            results = pool.map(
                lambda m: check_mutant(m, base, runs, work, args.steps, args.samples, args.full), selected)
            for mutant, killed, problems in results:
                expected = "(equivalent)" if mutant.equivalent else ", ".join(sorted(mutant.killers))
                print(f"| {mutant.ident} | {mutant.summary} | {expected} | {', '.join(sorted(killed)) or '-'} | "
                      f"{'; '.join(problems) or 'ok'} |", flush=True)
                failures.extend(f"{mutant.ident}: {p}" for p in problems)

        # (b) Witnesses.
        if not args.no_witnesses:
            witnessed = work / "witnesses.qnt"
            body = base.rstrip()
            assert body.endswith("}")
            extra = "".join(f"  val {name} = {expr}\n" for name, expr in WITNESSES.items())
            witnessed.write_text(body[:-1] + extra + "}\n")
            # `--witnesses` takes every following word, so the file goes first.
            out = quint("run", str(witnessed), "--max-steps=20", "--max-samples=10000", f"--seed={SEED}",
                        "--verbosity=2", "--witnesses", *WITNESSES)
            text = out.stdout + out.stderr
            counts = dict(re.findall(r"^(\w+) was witnessed in (\d+) trace", text, re.M))
            stats = re.search(r"Trace length statistics: (.*)", text)
            if out.returncode != 0 or set(counts) != set(WITNESSES):
                failures.append(f"witness run gave no count for every witness (exit {out.returncode}):\n{text}")
            print()
            print(f"witnesses (20 steps x 10000 traces, seed {SEED}; {stats.group(1) if stats else 'no stats'}):")
            for name in WITNESSES:
                count = int(counts.get(name, 0))
                print(f"  {name}: {count}")
                if count == 0:
                    failures.append(f"witness {name} never reached")
    finally:
        shutil.rmtree(work, ignore_errors=True)

    print()
    if failures:
        print("FAILED:")
        for failure in failures:
            print(f"  {failure}")
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
