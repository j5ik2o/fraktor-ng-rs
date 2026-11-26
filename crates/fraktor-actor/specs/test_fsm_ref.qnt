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

// The state a `TestFsmRef` reads and sets on a running `Fsm`
// (crates/fraktor-actor/src/kernel/actor/fsm.rs): the snapshot of the
// state name and data, `set_state` from outside the machine, the table
// of named timers, and the state timer with its generation.
//
// The model of issue #181 (design docs/design/issue-181-testfsmref.md,
// sections 3 to 6). It does not repeat `fsm_transition.qnt`, which
// models the transition notices to listeners.
//
// Firing is split into two actions on purpose: `timerDue` and
// `stateTimerDue` issue a mark, and `receiveNamedMark` and
// `receiveStateMark` receive it later. Between the two, `sendEvent`,
// `setState`, `startTimer`, `cancelTimer`, and `restart` may run, so a
// mark can arrive after its timer was cancelled, replaced, or outlived
// by a received message. The receiver judges a mark by its generation
// only, as the machine does; the entry IDs `eid` and `sid` are the
// model's own monotonic identifiers, which the invariants use to tell
// a stale mark from a current one even when generations collide.
//
// Real time is not modelled: a timer is due whenever the step says so.
// Names are elements of a set, not strings with contents.
module test_fsm_ref {
  type Row = { gen: int, repeat: bool, eid: int }
  type NamedMark = { name: str, gen: int, eid: int }
  type StateMark = { gen: int, sid: int }

  // States 1 and 2 have a state function; 9 has none.
  pure val States = Set(1, 2)
  pure val Targets = Set(1, 2, 9)
  pure val Datas = Set(0, 1)
  pure val Names = Set("a", "b")
  pure val InitState = 1
  pure val InitData = 0
  pure val NoName = "-"

  // The machine.
  var state: int
  var data: int
  var timers: str -> Row
  var generation: int
  var stActive: bool
  var stGen: int
  var stSid: int
  var stIssued: bool
  // The handle, which outlives a restart.
  var snapState: int
  var snapData: int
  var timerGen: int
  // Marks issued and not yet received.
  var namedMarks: Set[NamedMark]
  var stateMarks: Set[StateMark]
  // Model-only bookkeeping.
  var nextEid: int
  var nextSid: int
  var deadEids: Set[int]
  var liveSid: int
  var staleNamedAccepted: bool
  var staleStateAccepted: bool
  var acceptedSingle: str

  pure def moveTo(curState: int, curData: int, target: int, nextData: int): (int, int) =
    if (States.contains(target)) (target, nextData) else (curState, curData)

  pure def without(table: str -> Row, name: str): str -> Row =
    table.keys().exclude(Set(name)).mapBy(k => table.get(k))

  action init = all {
    state' = InitState,
    data' = InitData,
    timers' = Map(),
    generation' = 0,
    stActive' = false,
    stGen' = 0,
    stSid' = -1,
    stIssued' = false,
    snapState' = InitState,
    snapData' = InitData,
    timerGen' = 0,
    namedMarks' = Set(),
    stateMarks' = Set(),
    nextEid' = 0,
    nextSid' = 0,
    deadEids' = Set(),
    liveSid' = -1,
    staleNamedAccepted' = false,
    staleStateAccepted' = false,
    acceptedSingle' = NoName,
  }

  // The state function's answer to an ordinary message, after the receive
  // cancelled the state timer and raised the generation (I5). `target`
  // and `nextData` cover goto, stay, and `using`; a target with no state
  // function changes nothing. The snapshot is written whenever the
  // machine sets `current` (I1).
  action receiveOrdinary(nextGeneration: int, target: int, nextData: int): bool = {
    val moved = moveTo(state, data, target, nextData)
    all {
      state' = moved._1,
      data' = moved._2,
      snapState' = moved._1,
      snapData' = moved._2,
      generation' = nextGeneration,
      stActive' = false,
      liveSid' = -1,
      stGen' = stGen,
      stSid' = stSid,
      stIssued' = stIssued,
    }
  }

  action keepTimers = all {
    timers' = timers,
    timerGen' = timerGen,
    nextEid' = nextEid,
    deadEids' = deadEids,
  }

  action keepMarks = all {
    namedMarks' = namedMarks,
    stateMarks' = stateMarks,
  }

  action keepGhosts = all {
    nextSid' = nextSid,
    staleNamedAccepted' = staleNamedAccepted,
    staleStateAccepted' = staleStateAccepted,
  }

  action sendEvent(target: int, nextData: int): bool = all {
    receiveOrdinary(generation + 1, target, nextData),
    keepTimers,
    keepMarks,
    keepGhosts,
    acceptedSingle' = NoName,
  }

  // `set_state`: the elements left out keep the current value; like an
  // ordinary message it cancels the state timer and raises the generation
  // first, then moves as a reported goto, and arms a new state timer when
  // a timeout is given and the target exists (I2, I3, section 3.5).
  action setState(useName: bool, name: int, useData: bool, nextData: int, withTimeout: bool): bool = {
    val target = if (useName) name else state
    val wanted = if (useData) nextData else data
    val moved = moveTo(state, data, target, wanted)
    val arms = withTimeout and States.contains(target)
    all {
      state' = moved._1,
      data' = moved._2,
      snapState' = moved._1,
      snapData' = moved._2,
      generation' = generation + 1,
      stActive' = arms,
      stGen' = if (arms) generation + 1 else stGen,
      stSid' = if (arms) nextSid else stSid,
      stIssued' = if (arms) false else stIssued,
      liveSid' = if (arms) nextSid else -1,
      nextSid' = if (arms) nextSid + 1 else nextSid,
      keepTimers,
      keepMarks,
      staleNamedAccepted' = staleNamedAccepted,
      staleStateAccepted' = staleStateAccepted,
      acceptedSingle' = NoName,
    }
  }

  action keepMachine = all {
    state' = state,
    data' = data,
    snapState' = snapState,
    snapData' = snapData,
    generation' = generation,
    stActive' = stActive,
    stGen' = stGen,
    stSid' = stSid,
    stIssued' = stIssued,
    liveSid' = liveSid,
  }

  // Starting a timer replaces the one of the same name; its generation
  // comes from the one counter of the machine (section 3.2). It touches
  // neither the state timer nor the machine's generation.
  action startTimerWith(name: str, repeat: bool, gen: int): bool = all {
    timers' = timers.put(name, { gen: gen, repeat: repeat, eid: nextEid }),
    timerGen' = timerGen + 1,
    nextEid' = nextEid + 1,
    deadEids' = if (timers.keys().contains(name)) deadEids.union(Set(timers.get(name).eid)) else deadEids,
    keepMachine,
    keepMarks,
    keepGhosts,
    acceptedSingle' = NoName,
  }

  action startTimer(name: str, repeat: bool): bool = startTimerWith(name, repeat, timerGen + 1)

  action cancelTimer(name: str): bool = all {
    timers' = without(timers, name),
    deadEids' = if (timers.keys().contains(name)) deadEids.union(Set(timers.get(name).eid)) else deadEids,
    timerGen' = timerGen,
    nextEid' = nextEid,
    keepMachine,
    keepMarks,
    keepGhosts,
    acceptedSingle' = NoName,
  }

  action timerDue(name: str): bool = all {
    timers.keys().contains(name),
    namedMarks' = namedMarks.union(Set({ name: name, gen: timers.get(name).gen, eid: timers.get(name).eid })),
    stateMarks' = stateMarks,
    keepMachine,
    keepTimers,
    keepGhosts,
    acceptedSingle' = NoName,
  }

  action stateTimerDue = all {
    stActive,
    not(stIssued),
    stateMarks' = stateMarks.union(Set({ gen: stGen, sid: stSid })),
    namedMarks' = namedMarks,
    stIssued' = true,
    state' = state,
    data' = data,
    snapState' = snapState,
    snapData' = snapData,
    generation' = generation,
    stActive' = stActive,
    stGen' = stGen,
    stSid' = stSid,
    liveSid' = liveSid,
    keepTimers,
    keepGhosts,
    acceptedSingle' = NoName,
  }

  // Accepting a named mark is an ordinary receive: it cancels the state
  // timer and raises the generation (I5), removes a single timer from the
  // table (I9), and hands the timer's message to the state function.
  action acceptNamed(m: NamedMark, target: int, nextData: int): bool = {
    val single = timers.keys().contains(m.name) and not(timers.get(m.name).repeat)
    all {
      receiveOrdinary(generation + 1, target, nextData),
      timers' = if (single) without(timers, m.name) else timers,
      timerGen' = timerGen,
      nextEid' = nextEid,
      deadEids' = if (single) deadEids.union(Set(m.eid)) else deadEids,
      namedMarks' = namedMarks.exclude(Set(m)),
      stateMarks' = stateMarks,
      nextSid' = nextSid,
      staleNamedAccepted' = (
        staleNamedAccepted
          or deadEids.contains(m.eid)
          or not(timers.keys().contains(m.name) and timers.get(m.name).eid == m.eid)
      ),
      staleStateAccepted' = staleStateAccepted,
      acceptedSingle' = if (single) m.name else NoName,
    }
  }

  action dropNamed(m: NamedMark): bool = all {
    namedMarks' = namedMarks.exclude(Set(m)),
    stateMarks' = stateMarks,
    keepMachine,
    keepTimers,
    keepGhosts,
    acceptedSingle' = NoName,
  }

  // The receiver judges a named mark by its generation: the table must
  // hold the name with the mark's generation (I8).
  action receiveNamedMark(m: NamedMark, target: int, nextData: int): bool = all {
    namedMarks.contains(m),
    if (timers.keys().contains(m.name) and timers.get(m.name).gen == m.gen) acceptNamed(m, target, nextData)
    else dropNamed(m),
  }

  // Accepting the state mark checks the generation only. It neither
  // raises the generation nor empties the state timer's slot, so the
  // timer still reads as active until the next ordinary message (I5).
  action acceptState(m: StateMark, target: int, nextData: int): bool = {
    val moved = moveTo(state, data, target, nextData)
    all {
      state' = moved._1,
      data' = moved._2,
      snapState' = moved._1,
      snapData' = moved._2,
      generation' = generation,
      stActive' = stActive,
      stGen' = stGen,
      stSid' = stSid,
      stIssued' = stIssued,
      liveSid' = -1,
      keepTimers,
      namedMarks' = namedMarks,
      stateMarks' = stateMarks.exclude(Set(m)),
      nextSid' = nextSid,
      staleNamedAccepted' = staleNamedAccepted,
      staleStateAccepted' = (staleStateAccepted or m.sid != liveSid),
      acceptedSingle' = NoName,
    }
  }

  action dropState(m: StateMark): bool = all {
    stateMarks' = stateMarks.exclude(Set(m)),
    namedMarks' = namedMarks,
    keepMachine,
    keepTimers,
    keepGhosts,
    acceptedSingle' = NoName,
  }

  action receiveStateMark(m: StateMark, target: int, nextData: int): bool = all {
    stateMarks.contains(m),
    if (m.gen == generation) acceptState(m, target, nextData) else dropState(m),
  }

  // A failure in the state function: the failing message was received
  // first (the state timer cancelled, the generation raised), then the
  // new instance starts with the initial state and an empty table. The
  // counters on the handle are not reset (I10, section 3.2).
  action restart = all {
    state' = InitState,
    data' = InitData,
    snapState' = InitState,
    snapData' = InitData,
    generation' = generation + 1,
    stActive' = false,
    stGen' = stGen,
    stSid' = stSid,
    stIssued' = stIssued,
    liveSid' = -1,
    timers' = Map(),
    timerGen' = timerGen,
    nextEid' = nextEid,
    deadEids' = deadEids.union(timers.keys().map(k => timers.get(k).eid)),
    keepMarks,
    keepGhosts,
    acceptedSingle' = NoName,
  }

  // ---- The defects the negative controls run. ----

  // Accepts a state mark without the generation check.
  action bugStaleStateTimer(m: StateMark, target: int, nextData: int): bool = all {
    stateMarks.contains(m),
    acceptState(m, target, nextData),
  }

  // Cancels a timer without removing it from the table.
  action bugCancelKeepsTimer(name: str): bool = all {
    timers' = timers,
    deadEids' = if (timers.keys().contains(name)) deadEids.union(Set(timers.get(name).eid)) else deadEids,
    timerGen' = timerGen,
    nextEid' = nextEid,
    keepMachine,
    keepMarks,
    keepGhosts,
    acceptedSingle' = NoName,
  }

  // Judges on the sending side only: every mark that arrives is accepted.
  action bugSenderSideCheck(m: NamedMark, target: int, nextData: int): bool = all {
    namedMarks.contains(m),
    acceptNamed(m, target, nextData),
  }

  // Raises the generation only when the state name changes.
  action bugGenerationOnTransitionOnly(target: int, nextData: int): bool = {
    val moved = moveTo(state, data, target, nextData)
    all {
      receiveOrdinary(if (moved._1 != state) generation + 1 else generation, target, nextData),
      keepTimers,
      keepMarks,
      keepGhosts,
      acceptedSingle' = NoName,
    }
  }

  // Numbers each table row afresh, so a re-started name gets the
  // generation its cancelled predecessor had.
  action bugPerEntryGeneration(name: str, repeat: bool): bool = startTimerWith(name, repeat, 1)

  action step = {
    nondet target = Targets.oneOf()
    nondet nextData = Datas.oneOf()
    nondet name = Names.oneOf()
    nondet repeat = Set(true, false).oneOf()
    nondet useName = Set(true, false).oneOf()
    nondet useData = Set(true, false).oneOf()
    nondet withTimeout = Set(true, false).oneOf()
    any {
      sendEvent(target, nextData),
      setState(useName, target, useData, nextData, withTimeout),
      startTimer(name, repeat),
      cancelTimer(name),
      timerDue(name),
      stateTimerDue,
      restart,
      all {
        namedMarks != Set(),
        nondet m = namedMarks.oneOf()
        receiveNamedMark(m, target, nextData),
      },
      all {
        stateMarks != Set(),
        nondet m = stateMarks.oneOf()
        receiveStateMark(m, target, nextData),
      },
    }
  }

  // I1: the handle's snapshot is the machine's current state and data.
  val snapshotMatches = snapState == state and snapData == data
  // I4: at most one timer per name, each from its own generation.
  val oneTimerPerName = timers.keys().subseteq(Names)
    and timers.keys().forall(a => timers.keys().forall(b => a != b implies timers.get(a).gen != timers.get(b).gen))
  // I5: a state timeout is handled only while no ordinary message has
  // been received since its timer was armed.
  val stateTimeoutOnlyWhileLive = not(staleStateAccepted)
  // I8: a cancelled or replaced timer's mark is never handled as an event.
  val staleMarksAreDropped = not(staleNamedAccepted)
  // I9: a single timer is out of the table once its mark is accepted.
  val singleLeavesTable = acceptedSingle != NoName implies not(timers.keys().contains(acceptedSingle))

  val invariant = snapshotMatches
    and oneTimerPerName
    and stateTimeoutOnlyWhileLive
    and staleMarksAreDropped
    and singleLeavesTable

  // ---- Runs. ----

  // I2: a left-out element keeps its current value, in both directions.
  run setStateKeepsLeftOutElementsTest =
    init
      .then(setState(true, 2, false, 0, false))
      .expect(state == 2 and data == 0 and snapshotMatches)
      .then(setState(false, 0, true, 1, false))
      .expect(state == 2 and data == 1 and snapshotMatches)
      .then(setState(true, 9, true, 0, true))
      .expect(state == 2 and data == 1 and not(stActive))

  // I5: the state mark is accepted while live, and the slot still reads
  // as active afterwards; a stay then cancels it.
  run stateTimeoutFiresAndStaysActiveTest =
    init
      .then(setState(false, 0, false, 0, true))
      .then(stateTimerDue)
      .then(receiveStateMark({ gen: 1, sid: 0 }, 2, 1))
      .expect(state == 2 and data == 1 and stActive and invariant)
      .then(sendEvent(2, 1))
      .expect(not(stActive) and invariant)

  // I5, I8: a message received after the mark was issued makes it stale.
  run staleStateMarkIsDroppedTest =
    init
      .then(setState(false, 0, false, 0, true))
      .then(stateTimerDue)
      .then(sendEvent(1, 0))
      .then(receiveStateMark({ gen: 1, sid: 0 }, 2, 1))
      .expect(state == 1 and stateMarks == Set() and invariant)

  // I4, I8: the mark of a replaced timer is dropped; the new one's is not.
  run replacedTimerMarkIsDroppedTest =
    init
      .then(startTimer("a", false))
      .then(timerDue("a"))
      .then(startTimer("a", false))
      .then(receiveNamedMark({ name: "a", gen: 1, eid: 0 }, 2, 1))
      .expect(state == 1 and timers.keys().contains("a") and invariant)
      .then(timerDue("a"))
      .then(receiveNamedMark({ name: "a", gen: 2, eid: 1 }, 2, 1))
      .expect(state == 2 and not(timers.keys().contains("a")) and invariant)

  // I8: the mark of a cancelled timer is dropped.
  run cancelledTimerMarkIsDroppedTest =
    init
      .then(startTimer("a", true))
      .then(timerDue("a"))
      .then(cancelTimer("a"))
      .then(receiveNamedMark({ name: "a", gen: 1, eid: 0 }, 2, 1))
      .expect(state == 1 and namedMarks == Set() and timers == Map() and invariant)

  // I9: a repeating timer stays in the table after its mark is accepted.
  run repeatingTimerStaysTest =
    init
      .then(startTimer("b", true))
      .then(timerDue("b"))
      .then(receiveNamedMark({ name: "b", gen: 1, eid: 0 }, 1, 0))
      .expect(timers.keys().contains("b") and invariant)

  // I10: after a restart the old marks are dropped, a re-started name
  // gets a fresh generation, and the state starts over.
  run restartDropsOldMarksTest =
    init
      .then(setState(true, 2, true, 1, true))
      .then(startTimer("a", false))
      .then(stateTimerDue)
      .then(timerDue("a"))
      .then(restart)
      .expect(state == InitState and data == InitData and timers == Map() and invariant)
      .then(startTimer("a", false))
      .then(receiveNamedMark({ name: "a", gen: 1, eid: 0 }, 2, 1))
      .then(receiveStateMark({ gen: 1, sid: 0 }, 2, 1))
      .expect(state == InitState and timers.keys().contains("a") and invariant)

  run bugStaleStateTimerTest =
    init
      .then(setState(false, 0, false, 0, true))
      .then(stateTimerDue)
      .then(sendEvent(1, 0))
      .then(bugStaleStateTimer({ gen: 1, sid: 0 }, 2, 1))
      .expect(not(stateTimeoutOnlyWhileLive))

  run bugCancelKeepsTimerTest =
    init
      .then(startTimer("a", false))
      .then(timerDue("a"))
      .then(bugCancelKeepsTimer("a"))
      .then(receiveNamedMark({ name: "a", gen: 1, eid: 0 }, 2, 1))
      .expect(not(staleMarksAreDropped))

  run bugSenderSideCheckTest =
    init
      .then(startTimer("a", true))
      .then(timerDue("a"))
      .then(cancelTimer("a"))
      .then(bugSenderSideCheck({ name: "a", gen: 1, eid: 0 }, 2, 1))
      .expect(not(staleMarksAreDropped))

  run bugGenerationOnTransitionOnlyTest =
    init
      .then(setState(false, 0, false, 0, true))
      .then(stateTimerDue)
      .then(bugGenerationOnTransitionOnly(1, 0))
      .then(receiveStateMark({ gen: 1, sid: 0 }, 2, 1))
      .expect(not(stateTimeoutOnlyWhileLive))

  run bugPerEntryGenerationTest =
    init
      .then(bugPerEntryGeneration("a", false))
      .then(timerDue("a"))
      .then(cancelTimer("a"))
      .then(bugPerEntryGeneration("a", false))
      .then(receiveNamedMark({ name: "a", gen: 1, eid: 0 }, 2, 1))
      .expect(not(staleMarksAreDropped))
}
