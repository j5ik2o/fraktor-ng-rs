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
// sections 3, 4, and 6), aligned 1-to-1 with Pekko v2.0.0 `FSM.scala`:
// - Ordinary events and accepted named-timer marks cancel the armed state
//   timer, raise `stateGen`, run the state function (or `whenUnhandled` /
//   default warning + `stay()`), and re-arm the state timeout according to
//   `FSM.scala:906-912`:
//     1. `forMax(Infinite)` (`SomeMaxFiniteDuration`) disables the timeout.
//     2. `forMax(After(_))` arms a state timeout for the new `stateGen`.
//     3. Otherwise (`NoOverride`, including `stay()` and unhandled default)
//        arms the state's default `stateTimeout` if one is registered.
// - `TimeoutMarker` (`StateTimeoutMark`) checks `instance_id` and `stateGen`
//   without raising `stateGen` or clearing `stActive` (`isStateTimerActive`);
//   if the transition's target has a timeout (via `forMax(After)` or default
//   `stateTimeout`), a fresh deadline is armed under the current `stateGen`.
// - Named timers use a separate generation counter (`namedGen`) so starting or
//   cancelling a named timer (from a state function or `onTransition`) never
//   invalidates an armed state timer.
// - Restart creates a fresh `Fsm` instance with `stateGen = 0` and
//   `namedGen = 0`, distinguished from prior instances by a globally unique
//   `instance_id` (`owner eq this` in `FSM.scala:823`).
// - Stopping the actor cancels every timer immediately; no deadline fires
//   after stop.
// - A `goto` into a state that has no state function keeps the state, arms
//   no state timer, and clears the named timers (`terminate`,
//   `FSM.scala:886-888, 933-939`); the actor keeps running.
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

  // State timer slot (models Executor deadline + `isStateTimerActive`).
  var stActive: bool
  var stGen: int
  var stSid: int
  var stIssued: bool

  // Named timer table.
  var timers: str -> TimerRow

  // Marks already enqueued in the mailbox and not yet processed.
  var namedMarks: Set[NamedMark]
  var stateMarks: Set[StateMark]

  // Model-only ghost state for invariants.
  var nextEid: int
  var nextSid: int
  var deadEids: Set[int]
  var liveSid: int
  var lastOverride: TimeoutOverride
  var lastTarget: int
  var keptAfterMark: bool
  var staleStateAccepted: bool
  var staleNamedAccepted: bool
  var acceptedSingle: str
  // The `Single` timers whose mark has been accepted so far. A `Single` fires
  // at most once (I3), so this never grows a repeated name.
  var acceptedSingles: Set[str]

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
    namedMarks' = Set(),
    stateMarks' = Set(),
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
    acceptedSingles' = Set(),
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

  // Applies `makeTransition` for a state function result (or default unhandled
  // `stay()`). `actualArm` is what the transition arms and `expectedArm` is what
  // I1 selects; a negative control passes values that differ, which is where
  // its defect comes from. Nothing here compares the two: `armedMatchesSelection`
  // works the selection out again from the model's own state (review low 1).
  action applyTransitionWith(
    nextStateGen: int,
    kind: TransitionKind,
    target: int,
    ov: TimeoutOverride,
    fromStateTimeout: bool,
    actualArm: bool,
    expectedArm: bool
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
    val keepsSlot = fromStateTimeout and not(arms)
    all {
      state' = if (valid) effTarget else state,
      stateGen' = nextStateGen,
      stActive' = if (arms) true else (if (fromStateTimeout) keptActive else false),
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
    applyTransitionWith(nextStateGen, kind, target, ov, fromStateTimeout, shouldArm, shouldArm)
  }

  // Ordinary message: cancels armed state timer, raises `stateGen`, runs
  // state function / unhandled default (`stay()`), and re-arms per I1.
  action receiveOrdinary(kind: TransitionKind, target: int, ov: TimeoutOverride): bool = all {
    alive,
    applyTransition(stateGen + 1, kind, target, ov, false),
    keepLifecycle,
    timers' = if (gotoNonExistent(kind, target)) Map() else timers,
    deadEids' = if (gotoNonExistent(kind, target)) deadEids.union(eidsOf(timers)) else deadEids,
    namedGen' = namedGen,
    nextEid' = nextEid,
    keepMarks,
    keepStaleFlags,
    acceptedSingle' = NoName,
    acceptedSingles' = acceptedSingles,
  }

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
    acceptedSingle' = NoName,
    acceptedSingles' = acceptedSingles,
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
    acceptedSingle' = NoName,
    acceptedSingles' = acceptedSingles,
  }

  // Executor deadline fires for an active named timer and enqueues its mark.
  action namedTimerDue(name: str): bool = all {
    alive,
    timers.keys().contains(name),
    namedMarks' = namedMarks.union(Set({
      inst: curInst,
      name: name,
      gen: timers.get(name).gen,
      eid: timers.get(name).eid,
    })),
    stateMarks' = stateMarks,
    keepLifecycle,
    keepStateMachine,
    keepTimers,
    keepStaleFlags,
    acceptedSingle' = NoName,
    acceptedSingles' = acceptedSingles,
  }

  // Executor deadline fires for the currently armed state timer and enqueues
  // its mark once (`stIssued` prevents duplicate firing for the same arming).
  action stateTimerDue = all {
    alive,
    liveSid != -1,
    not(stIssued),
    stateMarks' = stateMarks.union(Set({ inst: curInst, gen: stGen, sid: stSid })),
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
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  action acceptNamed(m: NamedMark, kind: TransitionKind, target: int, ov: TimeoutOverride): bool = {
    val single = timers.keys().contains(m.name) and not(timers.get(m.name).repeat)
    all {
      applyTransition(stateGen + 1, kind, target, ov, false),
      timers' = if (gotoNonExistent(kind, target)) Map() else if (single) without(timers, m.name) else timers,
      namedGen' = namedGen,
      nextEid' = nextEid,
      deadEids' = if (gotoNonExistent(kind, target)) deadEids.union(eidsOf(timers)).union(Set(m.eid))
        else if (single) deadEids.union(Set(m.eid))
        else deadEids,
      namedMarks' = namedMarks.exclude(Set(m)),
      stateMarks' = stateMarks,
      keepLifecycle,
      staleStateAccepted' = staleStateAccepted,
      staleNamedAccepted' = (
        staleNamedAccepted
          or deadEids.contains(m.eid)
          or not(timers.keys().contains(m.name) and timers.get(m.name).eid == m.eid)
      ),
      acceptedSingle' = if (single) m.name else acceptedSingle,
      acceptedSingles' = if (single) acceptedSingles.union(Set(m.name)) else acceptedSingles,
    }
  }

  action dropNamed(m: NamedMark): bool = all {
    namedMarks' = namedMarks.exclude(Set(m)),
    stateMarks' = stateMarks,
    keepLifecycle,
    keepStateMachine,
    keepTimers,
    keepStaleFlags,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // Named timer mark reception (Pekko FSM.scala:822-833).
  action receiveNamedMark(m: NamedMark, kind: TransitionKind, target: int, ov: TimeoutOverride): bool = all {
    alive,
    namedMarks.contains(m),
    if (m.inst == curInst and timers.keys().contains(m.name) and timers.get(m.name).gen == m.gen)
      acceptNamed(m, kind, target, ov)
    else
      dropNamed(m),
  }

  action acceptState(m: StateMark, kind: TransitionKind, target: int, ov: TimeoutOverride): bool = all {
    applyTransition(stateGen, kind, target, ov, true),
    stateMarks' = stateMarks.exclude(Set(m)),
    namedMarks' = namedMarks,
    keepLifecycle,
    timers' = if (gotoNonExistent(kind, target)) Map() else timers,
    deadEids' = if (gotoNonExistent(kind, target)) deadEids.union(eidsOf(timers)) else deadEids,
    namedGen' = namedGen,
    nextEid' = nextEid,
    staleNamedAccepted' = staleNamedAccepted,
    staleStateAccepted' = (staleStateAccepted or m.sid != liveSid),
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  action dropState(m: StateMark): bool = all {
    stateMarks' = stateMarks.exclude(Set(m)),
    namedMarks' = namedMarks,
    keepLifecycle,
    keepStateMachine,
    keepTimers,
    keepStaleFlags,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // State timeout mark reception (Pekko FSM.scala:818-821).
  action receiveStateMark(m: StateMark, kind: TransitionKind, target: int, ov: TimeoutOverride): bool = all {
    alive,
    stateMarks.contains(m),
    if (m.inst == curInst and stActive and m.gen == stateGen)
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
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
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
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // ---- Negative-control defective actions ----

  // Defect 1: ignores `forMax(Infinite)` and arms the target state's default.
  action bugForMaxIgnored(target: int): bool = all {
    alive,
    applyTransitionWith(
      stateGen + 1,
      Goto,
      target,
      OverrideInfinite,
      false,
      defaultHasTimeout(target),
      false
    ),
    keepLifecycle,
    keepTimers,
    keepMarks,
    keepStaleFlags,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // Defect 2: `stay()` cancels the old timer and forgets to re-arm the state's
  // default `stateTimeout` (review H1).
  action bugStayDropsDefaultTimeout = all {
    alive,
    applyTransitionWith(
      stateGen + 1,
      Stay,
      state,
      NoOverride,
      false,
      false,
      defaultHasTimeout(state)
    ),
    keepLifecycle,
    keepTimers,
    keepMarks,
    keepStaleFlags,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // Defect 3: unhandled event skips `makeTransition(stay())` and leaves the
  // default `stateTimeout` unarmed (review H1, current `fsm.rs:587-590`).
  action bugUnhandledDoesNotRearm = all {
    alive,
    applyTransitionWith(
      stateGen + 1,
      Unhandled,
      state,
      NoOverride,
      false,
      false,
      defaultHasTimeout(state)
    ),
    keepLifecycle,
    keepTimers,
    keepMarks,
    keepStaleFlags,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // Defect 4: `goto(currentState)` skips re-arming the state timeout.
  action bugSameStateGotoDoesNotRearm = all {
    alive,
    applyTransitionWith(
      stateGen + 1,
      Goto,
      state,
      NoOverride,
      false,
      false,
      defaultHasTimeout(state)
    ),
    keepLifecycle,
    keepTimers,
    keepMarks,
    keepStaleFlags,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // Review low 1, first mutation: `NoOverride` never arms, so neither `stay()`
  // nor the unhandled default re-arms the state's default timeout. The old
  // `expectedArmed` comparison did not catch it (it agreed with the mutation),
  // so `armedMatchesSelection` no longer reads `expectedArmed`.
  action bugNoOverrideNeverArms = all {
    alive,
    applyTransitionWith(stateGen + 1, Stay, state, NoOverride, false, false, false),
    keepLifecycle,
    keepTimers,
    keepMarks,
    keepStaleFlags,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // Review low 1, second mutation: only the unhandled event skips the re-arm
  // (the current `fsm.rs:587-590` mistake), while every other `NoOverride`
  // transition arms the default as it should.
  action bugUnhandledOnlyNeverArms = all {
    alive,
    applyTransitionWith(stateGen + 1, Unhandled, state, NoOverride, false, false, false),
    keepLifecycle,
    keepTimers,
    keepMarks,
    keepStaleFlags,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // Defect 5: `stay()` leaves the old state timer armed without advancing
  // `stateGen`, so a stale mark issued before `stay()` still fires.
  // The ghost `liveSid` becomes -1 because an ordinary message arrived,
  // marking the prior timer stale, while the defective machine fails to
  // advance `stateGen` or clear `stActive`.
  action bugStayKeepsOldTimer = all {
    alive,
    state' = state,
    stateGen' = stateGen,
    stActive' = stActive,
    stGen' = stGen,
    stSid' = stSid,
    stIssued' = stIssued,
    nextSid' = nextSid,
    liveSid' = -1,
    lastOverride' = NoOverride,
    lastTarget' = 0,
    keptAfterMark' = false,
    keepLifecycle,
    keepTimers,
    keepMarks,
    keepStaleFlags,
    acceptedSingle' = acceptedSingle,
    acceptedSingles' = acceptedSingles,
  }

  // Defect 6: restart resets local counters to 0 without changing `instance_id`
  // (review M1), allowing an old instance's queued mark to match the new
  // instance's generation 1.
  action bugRestartReusesInstanceId = restartWith(curInst, nextInst)

  action step = {
    nondet target = Targets.oneOf()
    nondet kind = Set(Goto, Stay, Unhandled).oneOf()
    nondet ov = Set(NoOverride, OverrideAfter, OverrideInfinite).oneOf()
    nondet name = Names.oneOf()
    nondet repeat = Set(true, false).oneOf()
    any {
      receiveOrdinary(kind, target, ov),
      startNamedTimer(name, repeat),
      cancelNamedTimer(name),
      namedTimerDue(name),
      stateTimerDue,
      restartActor,
      stopActor,
      all {
        namedMarks != Set(),
        nondet m = namedMarks.oneOf()
        receiveNamedMark(m, kind, target, ov),
      },
      all {
        stateMarks != Set(),
        nondet m = stateMarks.oneOf()
        receiveStateMark(m, kind, target, ov),
      },
    }
  }

  // ---- Invariants ----

  // I1: the armed state timer slot matches what I1 selects. The selection is
  // written out here again from the model's own variables (`stActive`,
  // `lastTarget`, `lastOverride`) instead of being read from `expectedArmed`,
  // which the actions fill from the same call as the arming itself: comparing
  // the ghost with itself let two broken selections pass the simulation
  // (review low 1). It is also not written with `selectionShouldArm`: a broken
  // `selectionShouldArm` would then move both sides.
  val armedMatchesSelection =
    alive
    implies
      not(ValidStates.contains(lastTarget))
      or keptAfterMark
      or (stActive == (lastOverride == OverrideAfter or (lastOverride == NoOverride and defaultHasTimeout(lastTarget))))

  // I1: `forMax(Infinite)` never leaves a live state timer deadline armed.
  val infiniteNeverArms = (lastOverride == OverrideInfinite) implies (liveSid == -1)
  // I1, I2, M1: no stale state timeout mark is ever accepted.
  val staleStateNeverFires = not(staleStateAccepted)
  // I2, I3, M1: no stale named timer mark is ever accepted.
  val staleNamedNeverFires = not(staleNamedAccepted)
  // I4: a stopped actor has no armed state timer and an empty named timer table.
  val stoppedNeverArms = not(alive) implies (liveSid == -1 and not(stActive) and timers == Map())
  // I3: a single-shot named timer leaves the table once its mark is accepted.
  // The marker persists until the next ordinary message, start, cancel or
  // transition, so this is a real check and not a tautology (review low 1).
  val singleLeavesTable = acceptedSingle != NoName implies not(timers.keys().contains(acceptedSingle))
  // I3: a `Single` fires at most once. A repeated name never joins the set, so
  // its size is the number of distinct singles accepted and no name can be in
  // it twice.
  val singleFiresOnce = acceptedSingles.size() <= Names.size()

  val invariant = armedMatchesSelection
    and infiniteNeverArms
    and staleStateNeverFires
    and staleNamedNeverFires
    and stoppedNeverArms
    and singleLeavesTable
    and singleFiresOnce

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

  run namedTimerMarkCancelsStateTimerTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", false))
      .then(namedTimerDue("a"))
      .then(stateTimerDue)
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Goto, SWithTimeoutB, OverrideInfinite))
      .expect(state == SWithTimeoutB and not(timers.keys().contains("a")) and liveSid == -1 and invariant)
      .then(receiveStateMark({ inst: 0, gen: 1, sid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SWithTimeoutB and invariant)

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

  run gotoNonExistentClearsTimersTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(startNamedTimer("a", false))
      .then(startNamedTimer("b", true))
      .then(namedTimerDue("a"))
      .then(receiveOrdinary(Goto, SNonExistent, NoOverride))
      .expect(alive and state == SWithTimeoutA and timers == Map() and liveSid == -1 and invariant)
      .then(receiveNamedMark({ inst: 0, name: "a", gen: 1, eid: 0 }, Goto, SNoTimeout, NoOverride))
      .expect(state == SWithTimeoutA and namedMarks == Set() and invariant)

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

  run bugStayKeepsOldTimerTest =
    init
      .then(receiveOrdinary(Goto, SWithTimeoutA, NoOverride))
      .then(stateTimerDue)
      .then(bugStayKeepsOldTimer)
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
}
