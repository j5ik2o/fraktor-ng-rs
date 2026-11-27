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

// The shared handle that a `TestActorRef` reads one actor through
// (design docs/design/issue-182-test-actor-ref.md, sections 3 and 6).
//
// One actor is modelled: its lifecycle (pending, running, stopped), the
// generation of its instance, the abstract state of the current
// instance, and its mailbox. The mailbox holds user messages and
// probes. A probe is the numbered control message that a read of the
// underlying actor sends, as Pekko's `underlyingActor` asks with
// `InternalGetActor`; processing it records the processed number and
// takes the snapshot of the instance that processes it. A read is split
// in two: `startRead` sends the probe, and `finishRead` returns once the
// probe has been processed or the actor has stopped. While a read is
// ready to finish nothing else runs, because the reader checks after
// every step it drives.
//
// `directReceive` is `TestActorRef.receive`: the behaviour runs at once,
// outside the mailbox. When the actor runs on the calling-thread
// dispatcher (`ctd`), the system messages that call queued (a stop of
// itself) are processed before it returns; otherwise they wait for the
// mailbox's own run.
//
// Real time, message contents, the choice of the supervisor's directive,
// and other actors are not modelled: a restart or a stop happens
// whenever the step says so.
module test_actor_ref {
  type Item = { probe: bool, seq: int, before: int }

  pure val MaxSeq = 4
  pure val MaxUser = 4
  pure val MaxQueue = 3

  // Fixed for a run: whether the actor's dispatcher is the calling-thread one.
  var ctd: bool
  // The actor.
  var phase: str
  var generation: int
  var instState: int
  var mailbox: List[Item]
  var stopPending: bool
  // The handle, which outlives a restart.
  var processedSeq: int
  var lastSeq: int
  var snapState: int
  var snapGen: int
  var terminated: bool
  // Watchers learn of the stop through `Terminated`.
  var notified: bool
  // The reader.
  var reading: bool
  var target: int
  // Model-only bookkeeping.
  var userSent: int
  var userDone: int
  var rewound: bool
  var staleRead: bool
  var probeLeak: bool
  var overtaken: bool
  var directStopUnsettled: bool

  action init = all {
    nondet onCtd = Set(true, false).oneOf()
    ctd' = onCtd,
    phase' = "pending",
    generation' = 0,
    instState' = 0,
    mailbox' = List(),
    stopPending' = false,
    processedSeq' = 0,
    lastSeq' = 0,
    snapState' = 0,
    snapGen' = 0,
    terminated' = false,
    notified' = false,
    reading' = false,
    target' = 0,
    userSent' = 0,
    userDone' = 0,
    rewound' = false,
    staleRead' = false,
    probeLeak' = false,
    overtaken' = false,
    directStopUnsettled' = false,
  }

  pure def hasProbe(queue: List[Item]): bool =
    queue.indices().exists(i => queue[i].probe)

  pure def removeAt(queue: List[Item], at: int): List[Item] =
    range(0, queue.length()).foldl(List(), (acc, j) => if (j == at) acc else acc.append(queue[j]))

  val readReady = reading and (processedSeq >= target or terminated)

  action keepHandle = all {
    processedSeq' = processedSeq,
    lastSeq' = lastSeq,
    snapState' = snapState,
    snapGen' = snapGen,
    terminated' = terminated,
  }

  action keepReader = all {
    reading' = reading,
    target' = target,
  }

  action keepGhosts = all {
    rewound' = rewound,
    staleRead' = staleRead,
    probeLeak' = probeLeak,
    overtaken' = overtaken,
    directStopUnsettled' = directStopUnsettled,
  }

  action keepActor = all {
    phase' = phase,
    generation' = generation,
    instState' = instState,
    mailbox' = mailbox,
    stopPending' = stopPending,
  }

  // The actor's `pre_start` runs in its own run after the creation returned.
  action start = all {
    phase == "pending",
    phase' = "running",
    generation' = generation,
    instState' = 0,
    mailbox' = mailbox,
    stopPending' = stopPending,
    ctd' = ctd,
    keepHandle,
    keepReader,
    keepGhosts,
    notified' = notified,
    userSent' = userSent,
    userDone' = userDone,
  }

  // The stop is final: the handle is marked before any watcher is told,
  // and the probes still queued are dropped without a dead letter.
  action finishStop(dropProbes: bool, markHandle: bool): bool = all {
    phase' = "stopped",
    generation' = generation,
    instState' = instState,
    mailbox' = List(),
    stopPending' = false,
    ctd' = ctd,
    processedSeq' = processedSeq,
    lastSeq' = lastSeq,
    snapState' = snapState,
    snapGen' = snapGen,
    terminated' = markHandle,
    notified' = notified,
    keepReader,
    rewound' = rewound,
    staleRead' = staleRead,
    probeLeak' = (probeLeak or (not(dropProbes) and hasProbe(mailbox))),
    overtaken' = overtaken,
    directStopUnsettled' = directStopUnsettled,
    userSent' = userSent,
    userDone' = userDone,
  }

  // A start that fails stops the actor (the supervisor's decision).
  action startFails = all {
    phase == "pending",
    finishStop(true, true),
  }

  action sendUser = all {
    phase != "stopped",
    userSent < MaxUser,
    mailbox.length() < MaxQueue,
    mailbox' = mailbox.append({ probe: false, seq: 0, before: 0 }),
    userSent' = userSent + 1,
    phase' = phase,
    generation' = generation,
    instState' = instState,
    stopPending' = stopPending,
    ctd' = ctd,
    keepHandle,
    keepReader,
    keepGhosts,
    notified' = notified,
    userDone' = userDone,
  }

  // A read sends a new probe behind every message already queued. A read
  // of a stopped actor sends nothing and finishes by panicking.
  action startRead = all {
    not(reading),
    lastSeq < MaxSeq,
    mailbox.length() < MaxQueue,
    val seq = lastSeq + 1
    all {
      reading' = true,
      target' = seq,
      lastSeq' = seq,
      mailbox' = if (phase == "stopped") mailbox else mailbox.append({ probe: true, seq: seq, before: userSent }),
    },
    phase' = phase,
    generation' = generation,
    instState' = instState,
    stopPending' = stopPending,
    ctd' = ctd,
    processedSeq' = processedSeq,
    snapState' = snapState,
    snapGen' = snapGen,
    terminated' = terminated,
    keepGhosts,
    notified' = notified,
    userSent' = userSent,
    userDone' = userDone,
  }

  // The mailbox hands the item at `at` to the running actor. A probe is
  // taken by the executor before the behaviour, so a changed behaviour
  // or a stash cannot hold it back.
  action deliverAt(at: int): bool = all {
    phase == "running",
    not(stopPending),
    mailbox.length() > at,
    val got = mailbox[at]
    all {
      mailbox' = removeAt(mailbox, at),
      instState' = if (got.probe) instState else instState + 1,
      userDone' = if (got.probe) userDone else userDone + 1,
      processedSeq' = if (got.probe) got.seq else processedSeq,
      snapState' = if (got.probe) instState else snapState,
      snapGen' = if (got.probe) generation else snapGen,
      rewound' = (rewound or (got.probe and got.seq < processedSeq)),
      overtaken' = (overtaken or (got.probe and userDone < got.before)),
    },
    phase' = phase,
    generation' = generation,
    stopPending' = stopPending,
    ctd' = ctd,
    lastSeq' = lastSeq,
    terminated' = terminated,
    keepReader,
    staleRead' = staleRead,
    probeLeak' = probeLeak,
    directStopUnsettled' = directStopUnsettled,
    notified' = notified,
    userSent' = userSent,
  }

  action deliverHead = deliverAt(0)

  // `context.stop`, `PoisonPill`, or a stop from outside: the system queue
  // takes it before any user message.
  action requestStop = all {
    phase != "stopped",
    not(stopPending),
    stopPending' = true,
    phase' = phase,
    generation' = generation,
    instState' = instState,
    mailbox' = mailbox,
    ctd' = ctd,
    keepHandle,
    keepReader,
    keepGhosts,
    notified' = notified,
    userSent' = userSent,
    userDone' = userDone,
  }

  action processStop = all {
    stopPending,
    phase != "stopped",
    finishStop(true, true),
  }

  // A restart builds a new instance; the handle keeps its numbers.
  action restartWith(nextProcessed: int): bool = all {
    phase == "running",
    not(stopPending),
    generation' = generation + 1,
    instState' = 0,
    processedSeq' = nextProcessed,
    rewound' = (rewound or nextProcessed < processedSeq),
    phase' = phase,
    mailbox' = mailbox,
    stopPending' = stopPending,
    ctd' = ctd,
    lastSeq' = lastSeq,
    snapState' = snapState,
    snapGen' = snapGen,
    terminated' = terminated,
    keepReader,
    staleRead' = staleRead,
    probeLeak' = probeLeak,
    overtaken' = overtaken,
    directStopUnsettled' = directStopUnsettled,
    notified' = notified,
    userSent' = userSent,
    userDone' = userDone,
  }

  action restart = restartWith(processedSeq)

  // `post_restart` fails, and the supervisor stops the actor.
  action restartFails = all {
    phase == "running",
    not(stopPending),
    finishStop(true, true),
  }

  action notifyWatchersWhen(ready: bool): bool = all {
    ready,
    not(notified),
    notified' = true,
    keepActor,
    ctd' = ctd,
    keepHandle,
    keepReader,
    keepGhosts,
    userSent' = userSent,
    userDone' = userDone,
  }

  action notifyWatchers = notifyWatchersWhen(terminated)

  // `TestActorRef.receive`: the behaviour runs once, outside the mailbox,
  // and leaves the queued items where they are.
  action directReceiveWith(selfStop: bool, runSystem: bool): bool = all {
    not(reading),
    phase == "running",
    not(stopPending),
    val settlesNow = selfStop and ctd and runSystem
    all {
      phase' = if (settlesNow) "stopped" else phase,
      mailbox' = if (settlesNow) List() else mailbox,
      stopPending' = (selfStop and not(settlesNow)),
      terminated' = (terminated or settlesNow),
      probeLeak' = probeLeak,
      directStopUnsettled' = (directStopUnsettled or (selfStop and ctd and not(settlesNow))),
    },
    generation' = generation,
    instState' = instState + 1,
    ctd' = ctd,
    processedSeq' = processedSeq,
    lastSeq' = lastSeq,
    snapState' = snapState,
    snapGen' = snapGen,
    keepReader,
    rewound' = rewound,
    staleRead' = staleRead,
    overtaken' = overtaken,
    notified' = notified,
    userSent' = userSent,
    userDone' = userDone,
  }

  action directReceive(selfStop: bool): bool = directReceiveWith(selfStop, true)

  // The read returns: a stopped actor makes it panic ("underlying actor is
  // terminated"); otherwise it returns the snapshot its probe took.
  action finishRead = all {
    readReady,
    reading' = false,
    target' = target,
    staleRead' = (staleRead or (not(terminated) and not(snapGen == generation and snapState == instState))),
    keepActor,
    ctd' = ctd,
    keepHandle,
    rewound' = rewound,
    probeLeak' = probeLeak,
    overtaken' = overtaken,
    directStopUnsettled' = directStopUnsettled,
    notified' = notified,
    userSent' = userSent,
    userDone' = userDone,
  }

  action idle = all {
    keepActor,
    ctd' = ctd,
    keepHandle,
    keepReader,
    keepGhosts,
    notified' = notified,
    userSent' = userSent,
    userDone' = userDone,
  }

  action step =
    if (readReady) finishRead
    else any {
      start,
      startFails,
      sendUser,
      startRead,
      deliverHead,
      requestStop,
      processStop,
      restart,
      restartFails,
      notifyWatchers,
      nondet selfStop = Set(true, false).oneOf()
      directReceive(selfStop),
      idle,
    }

  // ---- Invariants. ----

  // I4: the processed number never goes back and never passes the last
  // probe sent; a probe is processed only after every user message sent
  // before it.
  val sequenceMonotonic = not(rewound) and processedSeq <= lastSeq
  val probesKeepOrder = not(overtaken)
  // I5, I6: a read that returns sees the instance that processes its probe.
  val readsSeeCurrent = not(staleRead)
  // I7: a read never waits for a probe that can no longer be processed.
  val readNeverStuck =
    not(reading) or processedSeq >= target or terminated
      or (phase != "stopped" and mailbox.indices().exists(i => mailbox[i].probe and mailbox[i].seq == target))
  // I7: the probes of a stopped actor are not published as dead letters.
  val noProbeLeak = not(probeLeak)
  // I7: the handle is marked before any watcher learns of the stop.
  val handleBeforeWatchers = not(notified) or terminated
  // I11: on the calling-thread dispatcher, a receive that stops the actor
  // has stopped it when it returns.
  val directStopSettles = not(directStopUnsettled)

  val invariant = sequenceMonotonic
    and probesKeepOrder
    and readsSeeCurrent
    and readNeverStuck
    and noProbeLeak
    and handleBeforeWatchers
    and directStopSettles

  // ---- Bugs the negative controls inject. ----

  // The stop publishes the queued probes as dead letters.
  action bugProbeToDeadLetters = all {
    stopPending,
    phase != "stopped",
    finishStop(false, true),
  }

  // The stop does not mark the handle, so a read waits for nothing.
  action bugStopWithoutMark = all {
    stopPending,
    phase != "stopped",
    finishStop(true, false),
  }

  // A read that does not send a probe and trusts the last number.
  action bugReadWithoutProbe = all {
    not(reading),
    reading' = true,
    target' = processedSeq,
    keepActor,
    ctd' = ctd,
    keepHandle,
    keepGhosts,
    notified' = notified,
    userSent' = userSent,
    userDone' = userDone,
  }

  // A restart that resets the processed number.
  action bugSequenceRewind = restartWith(0)

  // The control-aware queue lets the probe overtake user messages.
  action bugProbeOvertakes = deliverAt(1)

  // Watchers are told while the stop is still pending.
  action bugNotifyBeforeHandle = notifyWatchersWhen(stopPending)

  // `receive` returns without running the system queue it filled.
  action bugDirectSkipsSystemRun(selfStop: bool): bool = directReceiveWith(selfStop, false)

  // ---- Runs. ----

  // I4, I5: a read after two messages sees both of them.
  run readSeesProcessedMessagesTest =
    init
      .then(start)
      .then(sendUser)
      .then(sendUser)
      .then(startRead)
      .then(deliverHead)
      .then(deliverHead)
      .then(deliverHead)
      .expect(readReady and processedSeq == 1 and snapState == 2)
      .then(finishRead)
      .expect(not(reading) and invariant)

  // I4: a probe sent before the start waits for the start.
  run probeWaitsForStartTest =
    init
      .then(startRead)
      .expect(not(readReady) and readNeverStuck)
      .then(start)
      .then(deliverHead)
      .then(finishRead)
      .expect(processedSeq == 1 and snapGen == 0 and invariant)

  // I6: after a restart a new read sees the new instance, and the number
  // goes on from where it was.
  run restartReadSeesNewInstanceTest =
    init
      .then(start)
      .then(sendUser)
      .then(deliverHead)
      .then(startRead)
      .then(deliverHead)
      .then(finishRead)
      .then(restart)
      .then(startRead)
      .then(deliverHead)
      .then(finishRead)
      .expect(processedSeq == 2 and snapGen == 1 and snapState == 0 and invariant)

  // I7: a stop with a probe in flight ends the read instead of leaving it
  // waiting, and drops the probe without a dead letter.
  run stopWithProbeInFlightTest =
    init
      .then(start)
      .then(startRead)
      .then(requestStop)
      .then(processStop)
      .expect(readReady and terminated and invariant)
      .then(finishRead)
      .then(notifyWatchers)
      .expect(notified and invariant)

  // I7: a restart whose `post_restart` fails stops the actor; the read
  // panics instead of returning the old snapshot.
  run restartFailureStopsTest =
    init
      .then(start)
      .then(startRead)
      .then(restartFails)
      .expect(readReady and terminated and invariant)

  // I11: on the calling-thread dispatcher the actor has stopped when
  // `receive` returns; elsewhere the stop waits for its run.
  run directReceiveStopsOnCallingThreadTest =
    init
      .then(start)
      .then(directReceive(true))
      .expect(if (ctd) terminated and phase == "stopped" else stopPending and not(terminated))
      .expect(invariant)

  run bugProbeToDeadLettersTest =
    init
      .then(start)
      .then(startRead)
      .then(requestStop)
      .then(bugProbeToDeadLetters)
      .expect(not(noProbeLeak))

  run bugStopWithoutMarkTest =
    init
      .then(start)
      .then(startRead)
      .then(requestStop)
      .then(bugStopWithoutMark)
      .expect(not(readNeverStuck))

  run bugReadWithoutProbeTest =
    init
      .then(start)
      .then(startRead)
      .then(deliverHead)
      .then(finishRead)
      .then(restart)
      .then(bugReadWithoutProbe)
      .then(finishRead)
      .expect(not(readsSeeCurrent))

  run bugSequenceRewindTest =
    init
      .then(start)
      .then(startRead)
      .then(deliverHead)
      .then(finishRead)
      .then(bugSequenceRewind)
      .expect(not(sequenceMonotonic))

  run bugProbeOvertakesTest =
    init
      .then(start)
      .then(sendUser)
      .then(startRead)
      .then(bugProbeOvertakes)
      .expect(not(probesKeepOrder))

  run bugNotifyBeforeHandleTest =
    init
      .then(start)
      .then(requestStop)
      .then(bugNotifyBeforeHandle)
      .expect(not(handleBeforeWatchers))

  run bugDirectSkipsSystemRunTest =
    init
      .then(all { ctd' = true, phase' = "running", generation' = 0, instState' = 0, mailbox' = List(),
                  stopPending' = false, processedSeq' = 0, lastSeq' = 0, snapState' = 0, snapGen' = 0,
                  terminated' = false, notified' = false, reading' = false, target' = 0, userSent' = 0,
                  userDone' = 0, rewound' = false, staleRead' = false, probeLeak' = false,
                  overtaken' = false, directStopUnsettled' = false })
      .then(bugDirectSkipsSystemRun(true))
      .expect(not(directStopSettles))
}
