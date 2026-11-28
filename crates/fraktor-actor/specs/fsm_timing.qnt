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

// State timeout selection, named/state timer separation, and restart
// `instance_id` isolation for a production `Fsm`
// (`crates/fraktor-actor/src/kernel/actor/fsm.rs`).
//
// Model of issue #198 (design `docs/design/issue-198-fsm-timing-actor-ref.md`,
// sections 3, 4, and 6). It follows Pekko v2.0.0 `FSM.scala` (`receive`,
// `makeTransition`, `terminate`) except where listed under "Differences"
// below:
// - Ordinary events and accepted named-timer marks cancel the armed state
//   timer, raise `stateGen`, run the state function (or `whenUnhandled` /
//   default warning + `stay()`), and re-arm the state timeout according to
//   `FSM.scala:906-912`:
//     1. `forMax(Infinite)` (`SomeMaxFiniteDuration`) disables the timeout.
//     2. `forMax(After(_))` arms a state timeout for the new `stateGen`.
//     3. Otherwise (`NoOverride`, including `stay()` and unhandled default)
//        arms the state's default `stateTimeout` if one is registered.
// - A `TimeoutMarker` (`StateMark`) is accepted when its generation is the
//   current `stateGen` (`FSM.scala:818-819`). Accepting it neither raises
//   `stateGen` nor clears `stActive` (`isStateTimerActive`); if the
//   transition's target has a timeout (via `forMax(After)` or default
//   `stateTimeout`), a fresh deadline is armed under the current `stateGen`.
// - Named timers use a separate generation counter (`namedGen`) so starting or
//   cancelling a named timer (from a state function or `onTransition`) never
//   invalidates an armed state timer.
// - Restart creates a fresh `Fsm` instance with `stateGen = 0`,
//   `namedGen = 0`, and an empty state timer slot, distinguished from prior
//   instances by a globally unique `instance_id` (`owner eq this` in
//   `FSM.scala:823`).
// - Stopping the actor cancels every timer immediately; no deadline fires
//   after stop.
// - A `goto` into a state that has no state function keeps the state, arms
//   no state timer, and clears the named timers (`terminate`,
//   `FSM.scala:886-888, 933-939`); the actor keeps running. `terminate`
//   cancels the state timer's future without resetting it to `None`
//   (`FSM.scala:939`), so on the state timeout path `stActive`
//   (`isStateTimerActive`, `FSM.scala:636`) stays true.
//
// Differences from Pekko (design section 6 has the table):
// - A state timeout mark also carries the `instance_id` and is dropped when it
//   belongs to an earlier instance. Pekko's `TimeoutMarker(gen)` carries no
//   owner (`FSM.scala:818`); the check is design M1, stronger than Pekko.
// - Every `goto` into a nonexistent state and every stop clears the timers.
//   Pekko's `terminate` does nothing while `currentState.stopReason` is set
//   (`FSM.scala:934`), which a `goto` into a nonexistent state leaves behind
//   (`FSM.scala:888, 940`) until a transition succeeds. Stronger than Pekko;
//   #357 aligns it.
// - The production machine also drops a state timeout mark unless
//   `state_timer_armed == Some(generation)` (`fsm.rs:965`). Pekko has no such
//   guard and this model does not either: `currentGenStateMarkIsLive` shows it
//   is redundant, and with it a missing generation bump stays hidden on the
//   path of Pekko's case 8. The script
//   `scripts/quint_fsm_timing_mutants.py` checks the guarded form as mutant
//   G1, with `stActive` standing for `state_timer_armed` (#356).
// - A stop clears `stActive`. Pekko's `postStop` runs `terminate`
//   (`FSM.scala:929`), which cancels `timeoutFuture` without resetting it to
//   `None` (`:939`), so `isStateTimerActive` can stay true after a stop that
//   arrives while a state timer is armed or kept after a state timeout mark.
//   The production machine clears it too (`FsmHandle::clear`), and
//   `stoppedNeverArms` asks for it, so this is stronger than Pekko (design
//   section 6.4; mutant M13 breaks the stronger rule only). Restart is not
//   affected: Pekko's new instance starts with `None` as well.
// - A `Single` may fire more than once while it is in the table
//   (`namedTimerDue` only needs the row). Pekko's scheduler fires it once.
//   This is wider than Pekko; the extra marks are dropped after the first
//   acceptance.
// - Not modelled: real time and the order of deadlines, `FixedDelay` versus
//   `FixedRate` (both are `repeat`), replies (`replying`), transition
//   notifications, `whenUnhandled` swaps, the initial state's timeout armed by
//   `initialize()`, and subscriptions. Tests own those (design section 6).
//
// Single-file spec checked by `scripts/quint-check-specs.sh`.
module fsm_timing {
  type TimeoutOverride = NoOverride | OverrideAfter | OverrideInfinite
  type TransitionKind = Goto | Stay | Unhandled
  type TimerRow = { gen: int, repeat: bool, eid: int }
  type NamedMark = { inst: int, name: str, gen: int, eid: int }
  type StateMark = { inst: int, gen: int, sid: int }

  // States: 0 has no default stateTimeout; 1 and 2 have a default stateTimeout;
  // 9 has no registered state function.
  pure val SNoTimeout = 0
  pure val SWithTimeoutA = 1
  pure val SWithTimeoutB = 2
  pure val SNonExistent = 9
  pure val ValidStates = Set(SNoTimeout, SWithTimeoutA, SWithTimeoutB)
  pure val Targets = Set(SNoTimeout, SWithTimeoutA, SWithTimeoutB, SNonExistent)
  pure val Names = Set("a", "b")
  pure val NoName = "-"

  pure def defaultHasTimeout(s: int): bool =
    s == SWithTimeoutA or s == SWithTimeoutB

  // Selection rule I1 (Pekko FSM.scala:906-912).
  pure def selectionShouldArm(targetState: int, ov: TimeoutOverride): bool =
    match ov {
      | OverrideInfinite => false
      | OverrideAfter => true
      | NoOverride => defaultHasTimeout(targetState)
    }

  pure def without(table: str -> TimerRow, name: str): str -> TimerRow =
    table.keys().exclude(Set(name)).mapBy(k => table.get(k))

  pure def eidsOf(table: str -> TimerRow): Set[int] =
    table.keys().map(k => table.get(k).eid)

  // Drops the first occurrence of `x` from `xs`, keeping the rest in order.
  // The mailbox is a list because one deadline can put the same mark in it
  // twice, and a set would merge the two deliveries.
  pure def dropFirst(xs: List[a], x: a): List[a] =
    xs.foldl(
      { out: [], done: false },
      (acc, y) =>
        if (not(acc.done) and y == x) { out: acc.out, done: true }
        else { out: acc.out.append(y), done: acc.done }
    ).out

  pure def countOf(counts: int -> int, key: int): int =
    if (counts.keys().contains(key)) counts.get(key) else 0

  // A `goto` into a state without a state function makes Pekko call
  // `terminate` (FSM.scala:886-888), which cancels every named timer and the
  // state timer (FSM.scala:933-939) without stopping the actor.
  pure def gotoNonExistent(kind: TransitionKind, target: int): bool =
    kind == Goto and not(ValidStates.contains(target))

  // Actor lifecycle and instance identity.
  var alive: bool
  var curInst: int
  var nextInst: int

  // Machine state and separated generation counters.
  var state: int
  var stateGen: int
  var namedGen: int

  // The machine's state timer slot. `stActive` is Pekko's
  // `timeoutFuture.isDefined` (`isStateTimerActive`, `FSM.scala:636`). Every
  // received ordinary message and accepted named mark sets it to `None`
  // before the state function runs (`FSM.scala:822-827, 849-853`), so after
  // such a message it is true only if the transition arms a new deadline. A
  // state timeout mark leaves it as it was unless the transition arms (it
  // stays true, also into a nonexistent state, `FSM.scala:939`). Restart
  // starts a new instance with an empty slot (`restartWith`), as in Pekko.
  // Stop clears it (`stopActor`), which Pekko does not ("Differences" above).
  // `stGen` and `stSid` are the generation and the deadline of the last
  // arming.
  var stActive: bool
  var stGen: int
  var stSid: int

  // The executor's state timer row for this actor (`executor_value.rs`
  // `schedule_fsm_state_timer` / `cancel_fsm_state_timer`). `liveSid` is the
  // deadline of the arming that has not been cancelled, -1 if there is none;
  // `stIssued` says the executor already fired it. This is not a ghost:
  // `stateTimerDue` is enabled by it.
  var liveSid: int
  var stIssued: bool

  // Named timer table.
  var timers: str -> TimerRow

  // Marks already enqueued in the mailbox and not yet processed. Lists, not
  // sets: the same mark can be delivered twice (a fixed-rate timer that
  // catches up, or a deadline that fires twice), and the mailbox must keep
  // both deliveries so that what happens to the second one is observable
  // (review: a `Set` merged them; #342 for the state marks).
  var namedMarks: List[NamedMark]
  var stateMarks: List[StateMark]

  // Model-only ghost state for invariants.
  var nextEid: int
  var nextSid: int
  var deadEids: Set[int]
  var lastOverride: TimeoutOverride
  var lastTarget: int
  var keptAfterMark: bool
  var staleStateAccepted: bool
  var staleNamedAccepted: bool
  var acceptedSingle: str
  // How many marks each `Single` registration (`eid`, the identifier the timer
  // got when it was started) has had accepted. Keying by `eid` rather than by
  // name is what makes a second acceptance of the same registration visible:
  // a name seen twice is a double fire, while the same name started again is a
  // new `eid` (review: a name set could not tell the two apart).
  var acceptedSingles: int -> int
  // The repeating timer whose mark was accepted last, kept until the next
  // ordinary message, start, cancel, restart, stop, or table-clearing
  // transition (#343).
  var acceptedRepeat: str
  // How many marks the executor enqueued for each state timer deadline
  // (`sid`), and whether any deadline fired after the actor stopped (I4).
  var stateFires: int -> int
  var firedAfterStop: bool
  // Which action (and branch) took the last step. Only read by witnesses and
  // runs (`scripts/quint_fsm_timing_mutants.py`, check (b)).
  var lastAction: str

  action init = all {
    alive' = true,
    curInst' = 0,
    nextInst' = 1,
    state' = SNoTimeout,
    stateGen' = 0,
    namedGen' = 0,
    stActive' = false,
    stGen' = 0,
    stSid' = -1,
    stIssued' = false,
    timers' = Map(),
    namedMarks' = List(),
    stateMarks' = List(),
    nextEid' = 0,
    nextSid' = 0,
    deadEids' = Set(),
    liveSid' = -1,
    lastOverride' = NoOverride,
    lastTarget' = 0,
    keptAfterMark' = false,
    staleStateAccepted' = false,
    staleNamedAccepted' = false,
    acceptedSingle' = NoName,
    acceptedSingles' = Map(),
    acceptedRepeat' = NoName,
    stateFires' = Map(),
    firedAfterStop' = false,
    lastAction' = "init",
  }

  action keepLifecycle = all {
    alive' = alive,
    curInst' = curInst,
    nextInst' = nextInst,
  }

  action keepTimers = all {
    timers' = timers,
    namedGen' = namedGen,
    nextEid' = nextEid,
    deadEids' = deadEids,
  }

  action keepMarks = all {
    namedMarks' = namedMarks,
    stateMarks' = stateMarks,
  }

  action keepStateMachine = all {
    state' = state,
    stateGen' = stateGen,
    stActive' = stActive,
    stGen' = stGen,
    stSid' = stSid,
    stIssued' = stIssued,
    nextSid' = nextSid,
    liveSid' = liveSid,
    lastOverride' = lastOverride,
    lastTarget' = lastTarget,
    keptAfterMark' = keptAfterMark,
  }

  action keepStaleFlags = all {
    staleStateAccepted' = staleStateAccepted,
    staleNamedAccepted' = staleNamedAccepted,
  }

  action keepFireLog = all {
    stateFires' = stateFires,
    firedAfterStop' = firedAfterStop,
  }

  // Applies `makeTransition` for a state function result (or default unhandled
  // `stay()`). `actualArm` is whether the transition arms: `applyTransition`
  // passes what I1 selects, and a negative control passes something else,
  // which is where its defect comes from. Nothing here checks the choice:
  // `armedMatchesSelection` works the selection out again from the model's
  // own state (review low 1).
  action applyTransitionWith(
    nextStateGen: int,
    kind: TransitionKind,
    target: int,
    ov: TimeoutOverride,
    fromStateTimeout: bool,
    actualArm: bool
  ): bool = {
    val effTarget = if (kind == Goto) target else state
    val effOv = if (kind == Unhandled) NoOverride else ov
    val valid = ValidStates.contains(effTarget)
    val arms = valid and actualArm
    // Read the pre-state before any assignment: inside `all { }` a later
    // assignment would otherwise be visible to `stActive` below.
    val keptActive = stActive
    val keptSid = stSid
    val keptGen = stGen
    val keptIssued = stIssued
    val keptNextSid = nextSid
    // Whether the transition needs a fresh deadline, worked out from the
    // target and the override alone, independently of `actualArm` (review:
    // `keptAfterMark` must not excuse a needed re-arm that a mutation drops).
    val needsDeadline = selectionShouldArm(effTarget, effOv)
    // A state timeout mark leaves `timeoutFuture` as it was unless the
    // transition arms a new deadline (`FSM.scala:818-821`). A `goto` into a
    // state without a state function arms nothing, and `terminate` cancels the
    // future without resetting it (`FSM.scala:886-888, 939`), so the slot is
    // kept on that path as well (#344: the earlier model cleared it).
    val keepsSlot = fromStateTimeout and not(valid and needsDeadline)
    all {
      state' = if (valid) effTarget else state,
      stateGen' = nextStateGen,
      stActive' = if (arms) true else (if (keepsSlot) keptActive else false),
      stGen' = if (arms) nextStateGen else keptGen,
      stSid' = if (arms) keptNextSid else keptSid,
      stIssued' = if (arms) false else keptIssued,
      liveSid' = if (arms) keptNextSid else -1,
      nextSid' = if (arms) keptNextSid + 1 else keptNextSid,
      lastOverride' = effOv,
      lastTarget' = effTarget,
      keptAfterMark' = keepsSlot,
    }
  }

  action applyTransition(
    nextStateGen: int,
    kind: TransitionKind,
    target: int,
    ov: TimeoutOverride,
    fromStateTimeout: bool
  ): bool = {
    val effTarget = if (kind == Goto) target else state
    val effOv = if (kind == Unhandled) NoOverride else ov
    val shouldArm = selectionShouldArm(effTarget, effOv)
    applyTransitionWith(nextStateGen, kind, target, ov, fromStateTimeout, shouldArm)
  }

  // Ordinary message: cancels armed state timer, raises `stateGen`, runs
  // state function / unhandled default (`stay()`), and re-arms per I1.
  // `nextStateGen` is `stateGen + 1`; only a negative control passes another.
  action receiveOrdinaryWith(nextStateGen: int, kind: TransitionKind, target: int, ov: TimeoutOverride): bool = all {
    alive,
    applyTransition(nextStateGen, kind, target, ov, false),
    keepLifecycle,
    timers' = if (gotoNonExistent(kind, target)) Map() else timers,
    deadEids' = if (gotoNonExistent(kind, target)) deadEids.union(eidsOf(timers)) else deadEids,
    namedGen' = namedGen,
    nextEid' = nextEid,
    keepMarks,
    keepStaleFlags,
    keepFireLog,
    acceptedSingle' = NoName,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = NoName,
    lastAction' = "ordinary",
  }

  action receiveOrdinary(kind: TransitionKind, target: int, ov: TimeoutOverride): bool =
    receiveOrdinaryWith(stateGen + 1, kind, target, ov)

  // Starts or replaces a named timer (from a state function or `onTransition`).
  // Uses `namedGen` independently of `stateGen`, leaving any armed state timer
  // intact (I2, I3).
  action startNamedTimer(name: str, repeat: bool): bool = all {
    alive,
    timers' = timers.put(name, { gen: namedGen + 1, repeat: repeat, eid: nextEid }),
    namedGen' = namedGen + 1,
    nextEid' = nextEid + 1,
    deadEids' = if (timers.keys().contains(name)) deadEids.union(Set(timers.get(name).eid)) else deadEids,
    keepLifecycle,
    keepStateMachine,
    keepMarks,
    keepStaleFlags,
    keepFireLog,
    acceptedSingle' = NoName,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = NoName,
    lastAction' = if (timers.keys().contains(name)) "start:replace" else "start:new",
  }

  // Cancels a named timer without touching the state timer (I2, I3).
  action cancelNamedTimer(name: str): bool = all {
    alive,
    timers' = without(timers, name),
    deadEids' = if (timers.keys().contains(name)) deadEids.union(Set(timers.get(name).eid)) else deadEids,
    namedGen' = namedGen,
    nextEid' = nextEid,
    keepLifecycle,
    keepStateMachine,
    keepMarks,
    keepStaleFlags,
    keepFireLog,
    acceptedSingle' = NoName,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = NoName,
    lastAction' = if (timers.keys().contains(name)) "cancel:present" else "cancel:absent",
  }

  // Executor deadline fires for an active named timer and enqueues its mark.
  // The executor knows only its rows, not whether the actor is alive, so the
  // row is the only guard: a stop that left a row behind would fire here
  // (`stoppedNeverFires`).
  action namedTimerDue(name: str): bool = all {
    timers.keys().contains(name),
    namedMarks' = namedMarks.append({
      inst: curInst,
      name: name,
      gen: timers.get(name).gen,
      eid: timers.get(name).eid,
    }),
    stateMarks' = stateMarks,
    keepLifecycle,
    keepStateMachine,
    keepTimers,
    keepStaleFlags,
    stateFires' = stateFires,
    firedAfterStop' = (firedAfterStop or not(alive)),
    acceptedSingle' = NoName,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = acceptedRepeat,
    lastAction' = "namedDue",
  }

  // Executor deadline fires for the currently armed state timer and enqueues
  // its mark. `once` is true: the executor consumes the row when it fires, so
  // one arming fires once (`stIssued`). Only a negative control passes false.
  // As in `namedTimerDue`, the row is the only guard.
  action stateTimerDueWith(once: bool): bool = all {
    liveSid != -1,
    not(once) or not(stIssued),
    stateMarks' = stateMarks.append({ inst: curInst, gen: stGen, sid: stSid }),
    namedMarks' = namedMarks,
    stIssued' = true,
    state' = state,
    stateGen' = stateGen,
    stActive' = stActive,
    stGen' = stGen,
    stSid' = stSid,
    nextSid' = nextSid,
    liveSid' = liveSid,
    lastOverride' = lastOverride,
    lastTarget' = lastTarget,
    keptAfterMark' = keptAfterMark,
    keepLifecycle,
    keepTimers,
    keepStaleFlags,
    stateFires' = stateFires.put(stSid, countOf(stateFires, stSid) + 1),
    firedAfterStop' = (firedAfterStop or not(alive)),
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = acceptedRepeat,
    lastAction' = "stateDue",
  }

  action stateTimerDue = stateTimerDueWith(true)

  // Accepts a named timer mark (`FSM.scala:822-833`). `nextStateGen` is
  // `stateGen + 1` (`:828`); only a negative control passes another.
  action acceptNamedWith(
    nextStateGen: int,
    m: NamedMark,
    kind: TransitionKind,
    target: int,
    ov: TimeoutOverride
  ): bool = {
    val single = timers.keys().contains(m.name) and not(timers.get(m.name).repeat)
    val clears = gotoNonExistent(kind, target)
    all {
      applyTransition(nextStateGen, kind, target, ov, false),
      timers' = if (clears) Map() else if (single) without(timers, m.name) else timers,
      namedGen' = namedGen,
      nextEid' = nextEid,
      deadEids' = if (clears) deadEids.union(eidsOf(timers)).union(Set(m.eid))
        else if (single) deadEids.union(Set(m.eid))
        else deadEids,
      namedMarks' = dropFirst(namedMarks, m),
      stateMarks' = stateMarks,
      keepLifecycle,
      keepFireLog,
      staleStateAccepted' = staleStateAccepted,
      staleNamedAccepted' = (
        staleNamedAccepted
          or deadEids.contains(m.eid)
          or not(timers.keys().contains(m.name) and timers.get(m.name).eid == m.eid)
      ),
      acceptedSingle' = if (single) m.name else acceptedSingle,
      acceptedSingles' =
        if (single) acceptedSingles.put(m.eid, countOf(acceptedSingles, m.eid) + 1)
        else acceptedSingles,
      acceptedRepeat' = if (single or clears) NoName else m.name,
      lastAction' = "acceptNamed",
    }
  }

  action dropNamed(m: NamedMark): bool = all {
    namedMarks' = dropFirst(namedMarks, m),
    stateMarks' = stateMarks,
    keepLifecycle,
    keepStateMachine,
    keepTimers,
    keepStaleFlags,
    keepFireLog,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = acceptedRepeat,
    lastAction' =
      if (m.inst != curInst) "dropNamed:inst"
      else if (not(timers.keys().contains(m.name))) "dropNamed:absent"
      else "dropNamed:gen",
  }

  pure def namedMarkMatches(m: NamedMark, inst: int, table: str -> TimerRow): bool =
    m.inst == inst and table.keys().contains(m.name) and table.get(m.name).gen == m.gen

  // Named timer mark reception (Pekko FSM.scala:822-833).
  action receiveNamedMark(m: NamedMark, kind: TransitionKind, target: int, ov: TimeoutOverride): bool = all {
    alive,
    namedMarks.indices().exists(i => namedMarks[i] == m),
    if (namedMarkMatches(m, curInst, timers))
      acceptNamedWith(stateGen + 1, m, kind, target, ov)
    else
      dropNamed(m),
  }

  action acceptState(m: StateMark, kind: TransitionKind, target: int, ov: TimeoutOverride): bool = all {
    applyTransition(stateGen, kind, target, ov, true),
    stateMarks' = dropFirst(stateMarks, m),
    namedMarks' = namedMarks,
    keepLifecycle,
    timers' = if (gotoNonExistent(kind, target)) Map() else timers,
    deadEids' = if (gotoNonExistent(kind, target)) deadEids.union(eidsOf(timers)) else deadEids,
    namedGen' = namedGen,
    nextEid' = nextEid,
    keepFireLog,
    staleNamedAccepted' = staleNamedAccepted,
    staleStateAccepted' = (staleStateAccepted or m.sid != liveSid),
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = if (gotoNonExistent(kind, target)) NoName else acceptedRepeat,
    lastAction' = "acceptState",
  }

  action dropState(m: StateMark): bool = all {
    stateMarks' = dropFirst(stateMarks, m),
    namedMarks' = namedMarks,
    keepLifecycle,
    keepStateMachine,
    keepTimers,
    keepStaleFlags,
    keepFireLog,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = acceptedRepeat,
    lastAction' = if (m.inst != curInst) "dropState:inst" else "dropState:gen",
  }

  // State timeout mark reception (Pekko FSM.scala:818-821): the generation is
  // Pekko's only check, and the instance is design M1. The production machine
  // also checks `state_timer_armed` (`fsm.rs:965`); see the header.
  action receiveStateMark(m: StateMark, kind: TransitionKind, target: int, ov: TimeoutOverride): bool = all {
    alive,
    stateMarks.indices().exists(i => stateMarks[i] == m),
    if (m.inst == curInst and m.gen == stateGen)
      acceptState(m, kind, target, ov)
    else
      dropState(m),
  }

  // Restart builds a fresh `Fsm` instance: local counters restart at 0, while
  // `curInst` advances to a globally unique `instance_id` (M1). Old marks in
  // the mailbox remain in `namedMarks`/`stateMarks` to prove they are dropped.
  action restartWith(newInst: int, newNextInst: int): bool = all {
    alive,
    alive' = true,
    curInst' = newInst,
    nextInst' = newNextInst,
    state' = SNoTimeout,
    stateGen' = 0,
    namedGen' = 0,
    stActive' = false,
    stGen' = 0,
    stSid' = -1,
    stIssued' = false,
    timers' = Map(),
    nextEid' = nextEid,
    deadEids' = deadEids.union(timers.keys().map(k => timers.get(k).eid)),
    nextSid' = nextSid,
    liveSid' = -1,
    lastOverride' = NoOverride,
    lastTarget' = 0,
    keptAfterMark' = false,
    keepMarks,
    keepStaleFlags,
    keepFireLog,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = NoName,
    lastAction' = "restart",
  }

  action restartActor = restartWith(nextInst, nextInst + 1)

  // Stop clears every timer immediately; Executor drops every pending deadline
  // for this actor (I4).
  action stopActor = all {
    alive,
    alive' = false,
    curInst' = curInst,
    nextInst' = nextInst,
    state' = state,
    stateGen' = stateGen,
    namedGen' = namedGen,
    stActive' = false,
    stGen' = stGen,
    stSid' = stSid,
    stIssued' = false,
    timers' = Map(),
    nextEid' = nextEid,
    deadEids' = deadEids.union(timers.keys().map(k => timers.get(k).eid)),
    nextSid' = nextSid,
    liveSid' = -1,
    lastOverride' = NoOverride,
    lastTarget' = 0,
    keptAfterMark' = false,
    keepMarks,
    keepStaleFlags,
    keepFireLog,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = NoName,
    lastAction' = "stop",
  }

  // ---- Negative-control defective actions ----
  // Each one is a real action called with other arguments (one argument, or
  // two for the restart); none sets a ghost variable by hand (#344). The
  // numbers follow design section 6.3. Defects 2, 3, 4, 7 and 8 are separate
  // review findings but make the same step: a `NoOverride` transition back
  // into the current state that arms nothing (`kind` only decides the
  // effective target and override, which are the same for the three kinds
  // here). The ten controls tell six kinds of defect apart.

  action keepNegativeControlGhosts = all {
    keepLifecycle,
    keepTimers,
    keepMarks,
    keepStaleFlags,
    keepFireLog,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
    acceptedRepeat' = acceptedRepeat,
    lastAction' = "bug",
  }

  // Defect 1: ignores `forMax(Infinite)` and arms the target state's default.
  action bugForMaxIgnored(target: int): bool = all {
    alive,
    applyTransitionWith(stateGen + 1, Goto, target, OverrideInfinite, false, defaultHasTimeout(target)),
    keepNegativeControlGhosts,
  }

  // Defect 2: `stay()` cancels the old timer and forgets to re-arm the state's
  // default `stateTimeout` (review H1).
  action bugStayDropsDefaultTimeout = all {
    alive,
    applyTransitionWith(stateGen + 1, Stay, state, NoOverride, false, false),
    keepNegativeControlGhosts,
  }

  // Defect 3: unhandled event skips `makeTransition(stay())` and leaves the
  // default `stateTimeout` unarmed (review H1, the `fsm.rs:587-590` of the
  // time). The step is the one of defect 2.
  action bugUnhandledDoesNotRearm = all {
    alive,
    applyTransitionWith(stateGen + 1, Unhandled, state, NoOverride, false, false),
    keepNegativeControlGhosts,
  }

  // Defect 4: `goto(currentState)` skips re-arming the state timeout. The
  // step is the one of defect 2.
  action bugSameStateGotoDoesNotRearm = all {
    alive,
    applyTransitionWith(stateGen + 1, Goto, state, NoOverride, false, false),
    keepNegativeControlGhosts,
  }

  // Defect 7 (review low 1, first mutation): `NoOverride` never arms, so
  // neither `stay()` nor the unhandled default re-arms the state's default
  // timeout. The step is the one of defect 2.
  action bugNoOverrideNeverArms = all {
    alive,
    applyTransitionWith(stateGen + 1, Stay, state, NoOverride, false, false),
    keepNegativeControlGhosts,
  }

  // Defect 8 (review low 1, second mutation): only the unhandled event skips
  // the re-arm, while every other `NoOverride` transition arms the default as
  // it should. The step is the one of defect 3.
  action bugUnhandledOnlyNeverArms = all {
    alive,
    applyTransitionWith(stateGen + 1, Unhandled, state, NoOverride, false, false),
    keepNegativeControlGhosts,
  }

  // Defect 5: `stay()` does not advance `stateGen` (the ordinary message with
  // `nextStateGen = stateGen`), so a mark issued before `stay()` still carries
  // the current generation and is accepted (#344: the earlier control set the
  // ghost `liveSid` by hand instead).
  action bugStayKeepsOldTimer = receiveOrdinaryWith(stateGen, Stay, state, NoOverride)

  // Defect 6: restart resets local counters to 0 without changing `instance_id`
  // (review M1), allowing an old instance's queued mark to match the new
  // instance's generation 1.
  action bugRestartReusesInstanceId = restartWith(curInst, nextInst)

  // Defect 9 (#341, the M05 mutation): an accepted named mark does not advance
  // `stateGen`, so the state timeout mark that was already queued keeps the
  // current generation (Pekko's case 8, `FSMTimingSpec.scala:115-129`).
  action bugNamedMarkKeepsGeneration(m: NamedMark, kind: TransitionKind, target: int, ov: TimeoutOverride): bool = all {
    alive,
    namedMarks.indices().exists(i => namedMarks[i] == m),
    namedMarkMatches(m, curInst, timers),
    acceptNamedWith(stateGen, m, kind, target, ov),
  }

  // Defect 10 (#342, the M21 mutation): the executor fires one arming twice.
  action bugStateTimerFiresTwice = stateTimerDueWith(false)

  action step = {
    nondet target = Targets.oneOf()
    nondet kind = Set(Goto, Stay, Unhandled).oneOf()
    nondet ov = Set(NoOverride, OverrideAfter, OverrideInfinite).oneOf()
    nondet name = Names.oneOf()
    nondet repeat = Set(true, false).oneOf()
    // Restart and stop are drawn in a quarter of the steps. Stop is enabled in
    // every state and ends the trace, so drawing it as often as the other
    // branches kept the average trace near 6 steps (#344); the draw changes
    // how often a branch is tried, not which states can be reached.
    nondet lifecycleDraw = 0.to(3).oneOf()
    any {
      receiveOrdinary(kind, target, ov),
      startNamedTimer(name, repeat),
      cancelNamedTimer(name),
      namedTimerDue(name),
      stateTimerDue,
      all { lifecycleDraw == 0, any { restartActor, stopActor } },
      all {
        namedMarks != List(),
        nondet i = namedMarks.indices().oneOf()
        receiveNamedMark(namedMarks[i], kind, target, ov),
      },
      all {
        stateMarks != List(),
        nondet i = stateMarks.indices().oneOf()
        receiveStateMark(stateMarks[i], kind, target, ov),
      },
    }
  }

  // ---- Invariants ----

  // I1: the armed state timer slot matches what I1 selects, including a
  // `goto` into a nonexistent state. The selection is written out here again
  // from the model's own variables (`stActive`, `lastTarget`, `lastOverride`)
  // instead of being read from a ghost that the actions fill from the same
  // call as the arming itself (an earlier version's `expectedArmed`):
  // comparing the ghost with itself let two broken selections pass the
  // simulation (review low 1). It is also not written with
  // `selectionShouldArm`: a broken `selectionShouldArm` would then move both
  // sides.
  val armedMatchesSelection =
    alive
    implies
      (if (not(ValidStates.contains(lastTarget)))
         // A `goto` into a state without a state function arms nothing under
         // any override (`FSM.scala:886-888` calls `terminate` before the
         // timeout is read). The slot is what the receipt left: kept after a
         // state timeout mark (`FSM.scala:939`), cleared otherwise. Which of
         // the two it is moves with `keptAfterMark`, so the run
         // `stateTimeoutToNonExistentKeepsSlotTest` checks that (M25).
         (liveSid == -1 and stActive == keptAfterMark)
       else if (keptAfterMark)
            // A state timeout mark kept the consumed slot, which is legal
            // only when the transition needs no fresh deadline: the slot
            // stays active and no new deadline is armed (`FSM.scala:818-821`).
            (stActive and liveSid == -1
              and not(lastOverride == OverrideAfter or (lastOverride == NoOverride and defaultHasTimeout(lastTarget))))
          else (stActive == (lastOverride == OverrideAfter or (lastOverride == NoOverride and defaultHasTimeout(lastTarget)))))

  // I1: `forMax(Infinite)` never leaves a live state timer deadline armed.
  val infiniteNeverArms = (lastOverride == OverrideInfinite) implies (liveSid == -1)
  // I1, I2, M1: no stale state timeout mark is ever accepted.
  val staleStateNeverFires = not(staleStateAccepted)
  // I1, I2 (#341): a queued state timeout mark of the running instance that
  // carries the current generation comes from the deadline that is armed now.
  // This is what makes Pekko's generation-only check (`FSM.scala:818-819`)
  // enough, and it is checked on what is queued, not at acceptance: a
  // transition that forgets to advance `stateGen` breaks it in that step,
  // whatever the target arms and whether or not the mark is received later.
  val currentGenStateMarkIsLive =
    alive implies stateMarks.indices().forall(i =>
      (stateMarks[i].inst == curInst and stateMarks[i].gen == stateGen) implies stateMarks[i].sid == liveSid)
  // I2, I3, M1: no stale named timer mark is ever accepted.
  val staleNamedNeverFires = not(staleNamedAccepted)
  // I4: a stopped actor has no armed state timer and an empty named timer table.
  val stoppedNeverArms = not(alive) implies (liveSid == -1 and not(stActive) and timers == Map())
  // I4: no deadline fires after the actor stopped. The due actions are guarded
  // by the executor's rows only, so this checks what the stop left behind.
  val stoppedNeverFires = not(firedAfterStop)
  // I4 (#342): the executor fires each state timer deadline at most once.
  val stateTimerFiresOnce = stateFires.keys().forall(s => stateFires.get(s) <= 1)
  // I3: a single-shot named timer leaves the table once its mark is accepted.
  // The marker persists until the next ordinary message, start, cancel or
  // transition, so this is a real check and not a tautology (review low 1).
  val singleLeavesTable = acceptedSingle != NoName implies not(timers.keys().contains(acceptedSingle))
  // I3: a `Single` fires at most once. Every registration (`eid`) whose mark
  // was accepted exactly once, so a second acceptance of the same `eid` shows
  // here even though a re-started timer is a different `eid`.
  val singleFiresOnce = acceptedSingles.keys().forall(e => acceptedSingles.get(e) <= 1)
  // I3 (#343): a repeating timer stays in the table, still repeating, after
  // its mark is accepted, until it is cancelled or replaced.
  val repeatStaysInTable =
    acceptedRepeat != NoName
      implies (timers.keys().contains(acceptedRepeat) and timers.get(acceptedRepeat).repeat)

  val invariant = armedMatchesSelection
    and infiniteNeverArms
    and staleStateNeverFires
    and currentGenStateMarkIsLive
    and staleNamedNeverFires
    and stoppedNeverArms
    and stoppedNeverFires
    and stateTimerFiresOnce
    and singleLeavesTable
    and singleFiresOnce
    and repeatStaysInTable

  // ---- Positive tests ----

  run defaultTimeoutOnEnterAndStayTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .expect(state == SWithTimeoutA and liveSid == 0 and stActive and invariant)
      .then(receiveOrdinary(Stay, SWithTimeoutA, NoOverride))
      .expect(state == SWithTimeoutA and liveSid == 1 and stateGen == 2 and invariant)

  run unhandledRearmsDefaultTimeoutTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(stateTimerDue)
      .then(receiveOrdinary(Unhandled, SNoTimeout, OverrideInfinite))
      .expect(state == SWithTimeoutA and liveSid == 1 and stateGen == 2 and invariant)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SWithTimeoutA and invariant)

  run forMaxInfiniteDisablesDefaultTimeoutTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, OverrideInfinite))
      .expect(state == SWithTimeoutA and liveSid == -1 and not(stActive) and invariant)

  run forMaxAfterOverridesNoDefaultTest =
    init
      .then(receiveOrdinary(Goto, SNoTimeout, OverrideAfter))
      .expect(state == SNoTimeout and liveSid == 0 and stActive and invariant)
      .then(stateTimerDue)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SNoTimeout and liveSid == -1 and stActive and invariant)

  run sameStateGotoRearmsTimeoutTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(stateTimerDue)
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .expect(state == SWithTimeoutA and liveSid == 1 and stateGen == 2 and invariant)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SWithTimeoutA and invariant)

  run stateTimeoutRearmsWhenTargetHasDefaultTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(stateTimerDue)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Stay, SWithTimeoutA, NoOverride))
      .expect(state == SWithTimeoutA and stateGen == 1 and liveSid == 1 and not(stIssued) and invariant)
      .then(stateTimerDue)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 1 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SNoTimeout and liveSid == -1 and invariant)

  run namedTimerDoesNotInvalidateStateTimerTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", false))
      .then(stateTimerDue)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SNoTimeout and timers.keys().contains("a") and invariant)

  // Pekko's case 8 (`FSMTimingSpec.scala:115-129`, states `:262-277`): the
  // named mark is processed first and moves to a state that arms no state
  // timer (B under `forMax(Infinite)`; Pekko's target registers no timeout),
  // and the queued state timeout mark is dropped because the generation moved
  // on.
  run namedTimerMarkCancelsStateTimerTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", false))
      .then(namedTimerDue("a"))
      .then(stateTimerDue)
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Goto, SWithTimeoutB, OverrideInfinite))
      .expect(state == SWithTimeoutB and not(timers.keys().contains("a")) and liveSid == -1 and invariant)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SWithTimeoutB and lastAction == "dropState:gen" and invariant)

  // #341: the case 8 path where the target arms a new state timer. Only on
  // this path does a missing `stateGen` bump show through a guard on the slot
  // (`fsm.rs:965`): the new arming makes the slot active again, so only the
  // generation tells the queued mark from the new deadline.
  run namedMarkRearmDropsOldStateMarkTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", false))
      .then(namedTimerDue("a"))
      .then(stateTimerDue)
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Goto, SWithTimeoutB, NoOverride))
      .expect(state == SWithTimeoutB and stateGen == 2 and stActive and liveSid == 1 and invariant)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SWithTimeoutB and liveSid == 1 and stateMarks == List() and lastAction == "dropState:gen" and invariant)

  // #342: one arming fires once; the executor's row is gone after it fires.
  run stateTimerFiresOnceTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(stateTimerDue)
      .expect(stateMarks.length() == 1 and stateFires.get(0) == 1 and invariant)
      .then(stateTimerDue.fail())

  // Pekko's case 7 shape (`FSMTimingSpec.scala:102-113`, `:237-251`): a mark
  // already queued when the timer is cancelled and started again under the
  // same name is dropped (`FSM.scala:823`).
  run cancelledTimerQueuedMarkDroppedTest =
    init
      .then(startNamedTimer("a", false))
      .then(namedTimerDue("a"))
      .then(cancelNamedTimer("a"))
      .then(startNamedTimer("a", false))
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Goto, SWithTimeoutA, NoOverride))
      .expect(state == SNoTimeout and stateGen == 0 and timers.get("a").eid == 1 and namedMarks == List()
        and lastAction == "dropNamed:gen" and invariant)

  // #343, Pekko's case 9 shape (`FSMTimingSpec.scala:131-140`): a repeating
  // timer keeps its row after each accepted mark and stops only when it is
  // cancelled.
  run repeatedTimerKeepsFiringUntilCancelledTest =
    init
      .then(startNamedTimer("a", true))
      .then(namedTimerDue("a"))
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Stay, SNoTimeout, NoOverride))
      .expect(timers.keys().contains("a") and acceptedRepeat == "a" and invariant)
      .then(namedTimerDue("a"))
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Stay, SNoTimeout, NoOverride))
      .expect(timers.keys().contains("a") and lastAction == "acceptNamed" and invariant)
      .then(cancelNamedTimer("a"))
      .expect(timers == Map() and invariant)
      .then(namedTimerDue("a").fail())

  run restartInstanceIdDropsOldMarksTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", false))
      .then(stateTimerDue)
      .then(namedTimerDue("a"))
      .then(restartActor)
      .expect(curInst == 1 and stateGen == 0 and namedGen == 0 and invariant)
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", false))
      .expect(stateGen == 1 and namedGen == 1)
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Goto, SNoTimeout, NoOverride))
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SWithTimeoutA and timers.keys().contains("a") and invariant)

  run stopClearsAllTimersTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutB, NoOverride))
      .then(startNamedTimer("a", true))
      .then(stopActor)
      .expect(not(alive) and liveSid == -1 and timers == Map() and invariant)
      .then(namedTimerDue("a").fail())

  run gotoNonExistentClearsTimersTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", false))
      .then(startNamedTimer("b", true))
      .then(namedTimerDue("a"))
      .then(receiveOrdinary(Goto, SNonExistent, NoOverride))
      .expect(alive and state == SWithTimeoutA and timers == Map() and liveSid == -1 and invariant)
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SWithTimeoutA and namedMarks == List() and invariant)

  // A timer that catches up arms two deadlines, and both marks reach the
  // mailbox with the same name and generation. The first acceptance leaves the
  // second queued, so cancelling between them cannot hide it (review: a `Set`
  // merged the two deliveries into one).
  run duplicateDeliveryStaysQueuedTest =
    init
      .then(startNamedTimer("a", true))
      .then(namedTimerDue("a"))
      .then(namedTimerDue("a"))
      .expect(namedMarks.length() == 2)
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Stay, SNoTimeout, NoOverride))
      .expect(namedMarks.length() == 1 and invariant)
      .then(cancelNamedTimer("a"))
      .expect(namedMarks.length() == 1 and invariant)

  // A state timeout mark into a state without a state function does not
  // transition, arms nothing, and clears the named timers; `terminate`
  // cancels the state timer's future but leaves it defined
  // (`FSM.scala:886-888, 939`), so `isStateTimerActive` stays true (#344:
  // the earlier model and its review cleared it, against Pekko).
  run stateTimeoutToNonExistentKeepsSlotTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", true))
      .then(stateTimerDue)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNonExistent, NoOverride))
      .expect(state == SWithTimeoutA and stActive and liveSid == -1 and timers == Map() and stateGen == 1 and invariant)
      .then(stateTimerDue.fail())

  // A `goto` into a state without a state function arms nothing under
  // `forMax(After)` either (`FSM.scala:886-888`), on the state timeout path,
  // where the slot stays, and on the ordinary path, where it is cleared. No
  // deadline is left to fire (`liveSid == -1` is what enables `stateTimerDue`).
  run gotoNonExistentUnderForMaxAfterArmsNothingTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(stateTimerDue)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNonExistent, OverrideAfter))
      .expect(state == SWithTimeoutA and stActive and liveSid == -1 and invariant)
      .then(receiveOrdinary(Goto, SNonExistent, OverrideAfter))
      .expect(state == SWithTimeoutA and not(stActive) and liveSid == -1 and invariant)
      .then(stateTimerDue.fail())

  // A `Single` started again under the same name is a new registration
  // (`eid`), and each registration is accepted once. The counts are keyed by
  // `eid`, so a second acceptance of one registration would show (review).
  run singleRestartedUnderSameNameFiresOnceTest =
    init
      .then(startNamedTimer("a", false))
      .then(namedTimerDue("a"))
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Stay, SNoTimeout, NoOverride))
      .expect(acceptedSingles.get(0) == 1 and not(timers.keys().contains("a")) and invariant)
      .then(startNamedTimer("a", false))
      .expect(timers.get("a").eid == 1 and invariant)
      .then(namedTimerDue("a"))
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 2, eid: 1 }, Stay, SNoTimeout, NoOverride))
      .expect(acceptedSingles.get(0) == 1 and acceptedSingles.get(1) == 1 and invariant)

  // ---- Negative controls (each must violate its targeted invariant) ----

  run bugForMaxIgnoredTest =
    init
      .then(bugForMaxIgnored(SWithTimeoutA))
      .expect(not(infiniteNeverArms) and not(armedMatchesSelection))

  run bugStayDropsDefaultTimeoutTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(bugStayDropsDefaultTimeout)
      .expect(not(armedMatchesSelection))

  run bugUnhandledDoesNotRearmTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(bugUnhandledDoesNotRearm)
      .expect(not(armedMatchesSelection))

  run bugSameStateGotoDoesNotRearmTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(bugSameStateGotoDoesNotRearm)
      .expect(not(armedMatchesSelection))

  run bugNoOverrideNeverArmsTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutB, NoOverride))
      .then(bugNoOverrideNeverArms)
      .expect(not(armedMatchesSelection))

  run bugUnhandledOnlyNeverArmsTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutB, NoOverride))
      .then(bugUnhandledOnlyNeverArms)
      .expect(not(armedMatchesSelection))

  // The queued mark keeps the current generation the moment `stay()` forgets
  // to advance it, and receiving it accepts a mark of a cancelled deadline.
  run bugStayKeepsOldTimerTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(stateTimerDue)
      .then(bugStayKeepsOldTimer)
      .expect(stateGen == 1 and liveSid == 1 and not(currentGenStateMarkIsLive))
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SNoTimeout and not(staleStateNeverFires))

  run bugRestartReusesInstanceIdTest =
    init
      .then(startNamedTimer("a", false))
      .then(namedTimerDue("a"))
      .then(bugRestartReusesInstanceId)
      .then(startNamedTimer("a", false))
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Goto, SWithTimeoutA, NoOverride))
      .expect(not(staleNamedNeverFires))

  // #341 on the path of Pekko's case 8 itself (the target arms nothing): the
  // queued-mark invariant breaks in the step that forgets the bump, and the
  // mark is then accepted.
  run bugNamedMarkKeepsGenerationTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", false))
      .then(namedTimerDue("a"))
      .then(stateTimerDue)
      .then(bugNamedMarkKeepsGeneration({ inst: 0, name: "a", gen: 1, eid: 0 }, Goto, SWithTimeoutB, OverrideInfinite))
      .expect(state == SWithTimeoutB and not(stActive) and not(currentGenStateMarkIsLive))
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SNoTimeout and not(staleStateNeverFires))

  // #342: a second firing of one arming is counted, stays in the mailbox as a
  // second delivery, and is accepted after the first one kept the slot.
  run bugStateTimerFiresTwiceTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(stateTimerDue)
      .then(bugStateTimerFiresTwice)
      .expect(stateMarks.length() == 2 and not(stateTimerFiresOnce))
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(stateMarks.length() == 1 and not(currentGenStateMarkIsLive))
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(stateMarks == List() and not(staleStateNeverFires))
}
