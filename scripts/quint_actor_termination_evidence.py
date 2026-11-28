#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Rebuilds the evidence behind crates/fraktor-actor/specs/actor_termination.qnt.

Design of #234, section 11.13.17 ((a) and (b)). Every check works on a copy of
the spec in a temporary directory and never writes to the repository. Each
edit of a copy must match its spot exactly once, so a change of the spec that
moves a spot stops the script instead of measuring the unchanged spec.

  mutants    (a) a broken copy for each invariant that no mode breaks, and the
             postStopOnce group: the expected outcome is a violation, except
             for the postStopOnce singles and pairs that are expected to hold.
  modes      (a) `quint run --invariant=invariant` from each mistake's mode
             (the `init` of a copy starts in that mode).
  controls   (a) each negative-control run with its mode turned into Fixed
             (it must fail: the run depends on the mistake; the run of
             HandleRecreateUnderCreation only pins a state and passes), the
             boundary run of UntoldEscalation (it must pass), and a broad UntoldEscalation
             that also leaves an escalated failure untold (the boundary run
             must fail).
  witnesses  (b) how many traces take each action and each branch of runSys
             and receive, and how many reach the deep states.

Run from the repository root, after `npm ci`:

  python3 scripts/quint_actor_termination_evidence.py all
  python3 scripts/quint_actor_termination_evidence.py modes --repeat 3
  python3 scripts/quint_actor_termination_evidence.py witnesses --steps 40 --samples 50000

`--seed` fixes the seed of every `quint run` (with --repeat, the i-th run takes
seed + i), so a result can be measured again; without it each run takes a
random seed, and a violation prints the seed that reproduces it. Every quint
process runs one after another (at most one heavy check at a time).
"""

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

SPEC = Path("crates/fraktor-actor/specs/actor_termination.qnt")
QUINT = "./node_modules/.bin/quint"


class SpotError(Exception):
    pass


def edit(text, old, new):
    count = text.count(old)
    if count != 1:
        raise SpotError(f"expected one match, found {count}: {old[:70]!r}")
    return text.replace(old, new)


def edited(text, edits):
    for old, new in edits:
        text = edit(text, old, new)
    return text


def run_quint(args):
    result = subprocess.run([QUINT, *args], capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


def write_copy(work, name, text):
    path = Path(work) / f"{name}.qnt"
    path.write_text(text, encoding="utf-8")
    return path


def seed_args(seed, index):
    return [] if seed is None else [f"--seed={hex(seed + index)}"]


def simulate(path, invariant, steps, samples, seed, index=0, extra=()):
    # The path goes first: --witnesses takes every argument after it.
    code, out = run_quint([
        "run", str(path), f"--invariant={invariant}", f"--max-steps={steps}", f"--max-samples={samples}",
        *seed_args(seed, index), *extra,
    ])
    violated = "Found an issue" in out or "[violation]" in out
    if code != 0 and not violated:
        raise SystemExit(f"quint run failed on {path}:\n{out}")
    if seed is not None:
        return violated, hex(seed + index), out
    used = re.search(r"--seed=(0x[0-9a-f]+)", out)
    return violated, used.group(1) if used else "-", out


# ---- (a) broken copies --------------------------------------------------------

RECEIVE_GUARD = ("      isOpen(w, n),\n      a.suspend == 0,\n", "      isOpen(w, n),\n")
SEND_GUARD = ("    if (isOpen(x, n)) updA(x, n, a => { ...a, sys: a.sys.append(m) })",
              "    if (true) updA(x, n, a => { ...a, sys: a.sys.append(m) })")
RUN_GUARD = ("      isOpen(w, n),\n      a.sys.length() > 0,\n", "      a.sys.length() > 0,\n")
KEEP_INSTANCE = ("      ...a, phase: Closed, inst: false,", "      ...a, phase: Closed, inst: a.inst,")

# (invariant, what the copy breaks, edits, steps, samples, expect a violation)
MUTANTS = [
    ("heldNamesHaveActors", "releaseName frees the slot but keeps the name in the parent's table",
     [("    val x1 = setChildren(x, q, childrenOf(x, q).exclude(Set(c)))\n", "    val x1 = x\n")], 20, 10000, True),
    ("noReceiveWithoutInstance", "receive drops its guard a.suspend == 0", [RECEIVE_GUARD], 20, 10000, True),
    ("noReceiveWhileWaiting", "a Terminate that waits for children adds no suspend",
     [("toDie: y.children, wait: WTermination, suspend: y.suspend + 1 })",
       "toDie: y.children, wait: WTermination, suspend: y.suspend })")], 20, 10000, True),
    ("failedStartStaysSuspended", "failActor adds no suspend for a failed start",
     [("    val x1 = updA(x, n, y => { ...y, suspend: y.suspend + 1, failed:",
       "    val x1 = updA(x, n, y => { ...y, suspend: if (start) y.suspend else y.suspend + 1, failed:")],
     20, 10000, True),
    ("stashOnlyWhileSuspended", "unstashIfDown never gives back what the run kept aside",
     [("    if (isOpen(x, n) and stateOf(a) < before and a.stash.length() > 0)", "    if (false)")],
     20, 10000, True),
    ("postStopOnce", "both guards of a closed mailbox go (sendSys and runSys)", [SEND_GUARD, RUN_GUARD], 30, 20000, True),
    ("postStopOnce", "only the guard of sendSys goes", [SEND_GUARD], 30, 20000, False),
    ("postStopOnce", "only the guard of runSys goes", [RUN_GUARD], 30, 20000, False),
    ("postStopOnce", "finishTerminate keeps the instance", [KEEP_INSTANCE], 30, 20000, False),
    ("postStopOnce", "the guard of sendSys goes and finishTerminate keeps the instance",
     [SEND_GUARD, KEEP_INSTANCE], 30, 20000, False),
    ("postStopOnce", "the guard of runSys goes and finishTerminate keeps the instance",
     [RUN_GUARD, KEEP_INSTANCE], 30, 20000, False),
]


def check_mutants(spec, work, args):
    rows = []
    for index, (invariant, what, edits, steps, samples, expect) in enumerate(MUTANTS):
        path = write_copy(work, f"mutant{index}", edited(spec, edits))
        violated, seed, _ = simulate(path, invariant, steps, samples, args.seed, index)
        rows.append((invariant, what, f"{steps}/{samples}", "violated" if violated else "held", seed,
                     "ok" if violated == expect else "UNEXPECTED"))
    return ["invariant", "broken copy", "steps/samples", "result", "seed", "vs expected"], rows


def modes_of(spec):
    body = re.search(r"type Mode = (.*?)\n\n", spec, re.S).group(1)
    return [name.strip() for name in body.split("|") if name.strip() != "Fixed"]


def check_modes(spec, work, args):
    rows = []
    init = "    initWith(Fixed, afo)\n  }"
    for mode in modes_of(spec):
        path = write_copy(work, f"mode_{mode}", edit(spec, init, f"    initWith({mode}, afo)\n  }}"))
        found = 0
        seeds = []
        for index in range(args.repeat):
            violated, seed, _ = simulate(path, "invariant", args.steps, args.samples, args.seed, index)
            found += violated
            seeds.append(seed)
        rows.append((mode, f"{args.steps}/{args.samples}", f"{found}/{args.repeat}", " ".join(seeds)))
    return ["mode", "steps/samples", "violations", "seeds"], rows


# ---- (a) the runs of the mistakes ---------------------------------------------

BOUNDARY_RUNS = {"untoldEscalationStillTellsAnEscalatedFailureTest"}
# Runs of a mode that pass in Fixed as well: the run pins a state, and the
# model cannot reach the state where the mistake acts (design 11.13.11, the
# Creation wait).
PINS_ONLY_RUNS = {"handleRecreateUnderCreationRunsInsideTheWaitTest"}


def runs_of(spec):
    runs = re.findall(r"\n  run (\w+) =\n(.*?)(?=\n\n|\n}\s*$)", spec, re.S)
    declared = len(re.findall(r"^  run \w+ =", spec, re.M))
    if len(runs) != declared:
        raise SpotError(f"read {len(runs)} runs but the spec declares {declared}")
    return runs


def run_one_test(path, name):
    # A copy that does not typecheck would also fail its test; stop instead.
    code, out = run_quint(["typecheck", str(path)])
    if code != 0:
        raise SystemExit(f"the copy for {name} does not typecheck:\n{out}")
    code, out = run_quint(["test", f"--match=^{name}$", str(path)])
    passed = code == 0 and re.search(r"\b1 passing", out) is not None
    return passed, out


def check_controls(spec, work, args):
    rows = []
    for name, body in runs_of(spec):
        mode = re.search(r"initWith\((\w+), ", body).group(1)
        if mode == "Fixed":
            continue
        fixed_body = re.sub(r"initWith\(\w+, ", "initWith(Fixed, ", body, count=1)
        path = write_copy(work, f"control_{name}", edit(spec, body, fixed_body))
        passed, _ = run_one_test(path, name)
        expect_pass = name in BOUNDARY_RUNS or name in PINS_ONLY_RUNS
        rows.append((name, mode, "Fixed", "passes" if passed else "fails",
                     "ok" if passed == expect_pass else "UNEXPECTED"))
    broad = edit(spec, "md == UntoldEscalation and not(isAllForOne(x, n)) and not(escalated)",
                 "md == UntoldEscalation and not(isAllForOne(x, n))")
    for name in sorted(BOUNDARY_RUNS):
        passed, _ = run_one_test(write_copy(work, f"broad_{name}", broad), name)
        rows.append((name, "UntoldEscalation", "broad (escalation untold too)", "passes" if passed else "fails",
                     "ok" if not passed else "UNEXPECTED"))
    return ["run", "mode of the run", "copy", "result", "vs expected"], rows


# ---- (b) witnesses ------------------------------------------------------------

LABELS = '''
  def labelSpawn(n: str): str = {
    val q = PARENT_OF.get(n)
    if (q != ROOT and act(w, q).suspend > 0) "spawn_underASuspendedParent" else "spawn"
  }

  def labelSend(n: str): str = if (isOpen(w, n)) "send" else "send_toAClosedActor"

  def labelReceive(n: str, fails: bool, rd: Directive): str = {
    val a = act(w, n)
    if (not(fails and w.b.fails > 0 and a.inst)) "receive_ok"
    else if (PARENT_OF.get(n) != ROOT) "receive_failTellsTheParent"
    else match rd {
      | DRestart => if (w.b.restarts > 0) "receive_failGuardianRestarts" else "receive_failGuardianOverBudget"
      | DStop => "receive_failGuardianStops"
      | DEscalate => "receive_failGuardianDeciderFails"
    }
  }

  def labelRunSys(n: str, ok: bool, d: Directive, stopKids: bool): str = {
    val a = act(w, n)
    val m = if (a.sys.length() > 0) a.sys.head() else Create
    if (a.sys.length() == 0) "sys_nothing"
    else if (shouldStash(mode, m, stateOf(a))) "sys_keepAside"
    else match m {
      | Create =>
        if (a.wait == WTermination or a.handledCreate) "sys_createSkipped"
        else if (not(ok) and w.b.fails > 0) "sys_createFails" else "sys_createOk"
      | Terminate =>
        if (a.wait == WTermination) "sys_terminateSkipped"
        else if (a.children.size() > 0) "sys_terminateWaits" else "sys_terminateCloses"
      | Resume(byFailure) =>
        if (not(a.inst)) "sys_resumeWithoutInstance" else if (byFailure) "sys_resumeWithCause" else "sys_resumeWithoutCause"
      | Suspend => "sys_suspend"
      | Recreate =>
        if (not(a.inst)) (if (a.children.size() > 0) "sys_recreateWithoutInstanceWaits" else "sys_recreateWithoutInstanceCreates")
        else if (a.toDie.union(if (stopKids) a.children else Set()).size() > 0) "sys_recreateWaits"
        else if (not(ok) and w.b.fails > 0) "sys_recreatePostRestartFails" else "sys_recreateAtOnce"
      | Failed(f) =>
        if (not(a.children.contains(f.ref.name) and act(w, f.ref.name).inc == f.ref.inc)) "sys_failedDropped"
        else match d {
          | DRestart =>
            if (w.b.restarts == 0) "sys_failedRestartOverBudget"
            else if (isAllForOne(w, n)) "sys_failedAllForOneRestart" else "sys_failedRestart"
          | DStop => "sys_failedStop"
          | DEscalate => "sys_failedDeciderFails"
        }
      | ChildTerm(r) =>
        if (a.wait == WNone or a.toDie.exclude(Set(r.name)).size() > 0) "sys_childTerm"
        else match a.wait {
          | WTermination => "sys_childTermFinishesTerminate"
          | WCreation => "sys_childTermFinishesCreate"
          | WRecreation => "sys_childTermFinishesRecreate"
          | WNone => "sys_childTerm"
        }
    }
  }

'''

STEP = "    any { spawn(n), send(n), stop(n), runSys(n, ok, d, rd, stopKids), receive(n, fails, rd), tellRest(n) }"
LABELLED_STEP = """    any {
      all { spawn(n), last' = labelSpawn(n) },
      all { send(n), last' = labelSend(n) },
      all { stop(n), last' = "stop" },
      all { runSys(n, ok, d, rd, stopKids), last' = labelRunSys(n, ok, d, stopKids) },
      all { receive(n, fails, rd), last' = labelReceive(n, fails, rd) },
      all { tellRest(n), last' = "tellRest" },
    }"""

DEEP_STATES = {
    "deep_waitsInCreation": 'ACTORS.exists(n => act(w, n).wait == WCreation)',
    "deep_waitsInRecreation": 'ACTORS.exists(n => act(w, n).wait == WRecreation)',
    "deep_waitsInTermination": 'ACTORS.exists(n => act(w, n).wait == WTermination)',
    "deep_keepsARecreateWhileWaitingInRecreation":
        'ACTORS.exists(n => act(w, n).wait == WRecreation and act(w, n).stash.select(m => m == Recreate).length() > 0)',
    "deep_secondRestartOfOneIncarnation": 'ACTORS.exists(n => act(w, n).restarts >= 2)',
    "deep_postRestartFailed": 'ACTORS.exists(n => act(w, n).fatal)',
    "deep_escalationPerpetrator": 'ACTORS.exists(n => act(w, n).failed != "" and act(w, n).failed != n)',
    "deep_suspendedTwice": 'ACTORS.exists(n => isOpen(w, n) and act(w, n).suspend >= 2)',
}


def instrumented(spec):
    # sys_nothing only makes labelRunSys total (runSys needs a system message).
    labels = sorted(set(re.findall(r'"((?:spawn|send|receive|sys)_?\w*)"', LABELS)) - {"sys_nothing"} | {"stop", "tellRest"})
    actions = {
        "action_spawn": [x for x in labels if x.startswith("spawn")],
        "action_send": [x for x in labels if x.startswith("send")],
        "action_stop": ["stop"],
        "action_runSys": [x for x in labels if x.startswith("sys_")],
        "action_receive": [x for x in labels if x.startswith("receive")],
        "action_tellRest": ["tellRest"],
    }
    witnesses = {f"label_{x}": f'last == "{x}"' for x in labels}
    witnesses.update({k: " or ".join(f'last == "{x}"' for x in v) for k, v in actions.items()})
    witnesses.update(DEEP_STATES)
    head = spec[: spec.index("  // ---- runs: the fixed design, stage 4a")]
    head = edited(head, [
        ("  var w: World\n", "  var w: World\n  var last: str\n"),
        ("  action init = {\n    nondet afo = Bool.oneOf()\n    initWith(Fixed, afo)\n  }",
         LABELS + "  action init = {\n    nondet afo = Bool.oneOf()\n    all { initWith(Fixed, afo), last' = \"init\" }\n  }"),
        (STEP, LABELLED_STEP),
    ])
    defs = "".join(f"  val {name} = {expr}\n" for name, expr in witnesses.items())
    return head + "\n" + defs + "}\n", actions, list(witnesses)


def check_witnesses(spec, work, args):
    text, actions, names = instrumented(spec)
    path = write_copy(work, "witnesses", text)
    violated, seed, out = simulate(path, "invariant", args.steps, args.samples, args.seed, 0,
                                   extra=["--witnesses", *names])
    if violated:
        raise SystemExit(f"the instrumented copy violates invariant (seed {seed}):\n{out}")
    counts = dict(re.findall(r"(\w+) was witnessed in (\d+) trace", out))
    rows = [(name, counts.get(name, "?"), f"{100 * int(counts.get(name, 0)) / args.samples:.2f}%") for name in names]
    return [f"witness ({args.steps} steps, {args.samples} samples, seed {seed})", "traces", "share"], rows


# ---- driver -------------------------------------------------------------------

CHECKS = {"mutants": check_mutants, "modes": check_modes, "controls": check_controls, "witnesses": check_witnesses}


def print_table(title, header, rows):
    print(f"\n## {title}\n")
    print("| " + " | ".join(header) + " |")
    print("|" + "|".join(":---" for _ in header) + "|")
    for row in rows:
        print("| " + " | ".join(str(cell) for cell in row) + " |")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("check", choices=[*CHECKS, "all"])
    parser.add_argument("--seed", type=lambda text: int(text, 0), default=None)
    parser.add_argument("--repeat", type=int, default=1, help="quint runs per mode (modes only)")
    parser.add_argument("--steps", type=int, default=20, help="max steps (modes, witnesses)")
    parser.add_argument("--samples", type=int, default=10000, help="max samples (modes, witnesses)")
    parser.add_argument("--spec", type=Path, default=SPEC, help="the spec to copy (default: %(default)s)")
    args = parser.parse_args()
    spec = args.spec.read_text(encoding="utf-8")
    unexpected = False
    with tempfile.TemporaryDirectory() as work:
        for name in CHECKS if args.check == "all" else [args.check]:
            try:
                header, rows = CHECKS[name](spec, work, args)
            except SpotError as err:
                raise SystemExit(f"{name}: the spec moved under a spot of this script: {err}")
            print_table(name, header, rows)
            unexpected |= any("UNEXPECTED" in row for row in rows)
    sys.exit(1 if unexpected else 0)


if __name__ == "__main__":
    main()
