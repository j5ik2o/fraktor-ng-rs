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

// The start and the stop of actors in a tree, once both are system messages
// (stage 4a of #234: docs/design/issue-234-multithread-executor.md, section
// 11.12). It mirrors Pekko's ActorCell: `actorOf` reserves the child's name in
// the parent and puts Create on the child's own system queue
// (dungeon/Children.makeChild, dungeon/Dispatch.init); Create builds the
// instance and runs preStart (ActorCell.create); Terminate stops the children
// and waits for them (ActorCell.terminate); the last child's notice lets the
// parent go on with what it waited for (FaultHandling.handleChildTerminated:
// finishCreate, finishRecreate, finishTerminate); finishTerminate runs
// postStop, closes the mailbox, tells the parent and then the watchers.
//
// One step of an actor's run handles one system message (runSys) or one
// normal message (receive). Steps of different actors interleave freely, as
// threads of a pool would. How one mailbox is scheduled, and that a run
// handles every waiting system message before the next normal message, is
// mailbox_scheduling.qnt; here a normal message is taken only while the
// system queue is empty and the mailbox is not suspended, which is what the
// mailbox gives.
//
// The tree has fixed slots: the guardian (ROOT, no mailbox: the executor
// handles its children's notices at once) holds p; p holds c1 and c2; c1 holds
// g. A slot is one name, and a later incarnation reuses it once the name is
// free again. p watches c1, and an outside watcher watches every actor.
//
// What the executor does after a run (stage 4a keeps supervision there until
// stage 4b) is `decide`: Restart, Stop, or Escalate (the decider fails, and the
// parent is restarted). A restart waits for the children its preRestart
// stopped before postRestart (Recreation). A retried start first stops the
// children of the failed attempt and waits for them (Creation). The mailbox
// of an actor that failed is suspended until the decision's effect is over.
// An actor that failed to start has no instance, so only the retried Create
// resumes it.
//
// The modes other than Fixed are the mistakes the design avoids:
//   CloseWithChildren      : Terminate closes the mailbox without waiting for
//                            the children (T1).
//   NotifyBeforePostStop   : the watchers learn of the stop when Terminate
//                            arrives, before postStop and the close (T2, T3).
//   WatchersFirst          : finishTerminate tells the watchers before the
//                            parent (T4: a parent that also watches must
//                            handle the child's end before its Terminated).
//   EarlyNameRelease       : the name is freed when Terminate arrives, while
//                            the actor still waits for its own children.
//   SyncStop               : the executor stops an actor outside its run, as
//                            stage 3 did, before its Create has been handled.
//   KeepWatching           : a terminating actor keeps watching (T1's unwatch
//                            is missing), so a Terminated reaches an actor
//                            that is stopping.
//   ResumeAfterStartFailure: the decision on a failed start also sends the
//                            Resume that stage 3 sends after a decision.
//   ResumeOnRestart        : a restart is resumed by a Resume the decision
//                            sends at once (stage 3), not by finishRecreate.
//   NoCreationWait         : a retried start creates the instance while the
//                            children of the failed attempt are still there.
//   NoRecreationWait       : postRestart runs before the children that
//                            preRestart stopped have gone.
//   CreateInsideRestart    : a parent's restart retries a child's failed start
//                            inside itself, so a second failure of the child
//                            restarts the parent inside its own restart.
//   NoTerminateToChildren  : Terminate waits for children it never asked to
//                            stop.
//
// CI runs 20 steps (scripts/quint-check-specs.sh). For deeper runs:
//   quint run --invariant=invariant --max-steps=40 --max-samples=50000 \
//     crates/fraktor-actor/specs/actor_termination.qnt
module actor_termination {
  pure val ROOT = "root"
  pure val ACTORS = Set("p", "c1", "c2", "g")
  pure val PARENT_OF = Map("p" -> ROOT, "c1" -> "p", "c2" -> "p", "g" -> "c1")
  // p watches c1.
  pure val WATCHER = "p"
  pure val WATCHED = "c1"
  pure val SPAWNS = 6
  pure val STOPS = 2
  pure val FAILS = 2
  pure val MESSAGES = 2
  pure val RESTARTS = 2

  type Mode = Fixed | CloseWithChildren | NotifyBeforePostStop | WatchersFirst | EarlyNameRelease | SyncStop
    | KeepWatching | ResumeAfterStartFailure | ResumeOnRestart | NoCreationWait | NoRecreationWait
    | CreateInsideRestart | NoTerminateToChildren

  // Free: no incarnation holds the name. Unborn: Create waits. StartFailed:
  // Create failed and the actor has no instance. Closed: finishTerminate ran;
  // the name stays taken until the parent has handled the child's notice.
  type Phase = Free | Unborn | Alive | StartFailed | Closed
  // What the actor waits for its children for (Pekko's SuspendReason).
  type Wait = WNone | WCreation | WRecreation | WTermination
  type Ref = { name: str, inc: int }
  type Sys = Create | Terminate | Resume | ChildTerm(Ref)
  type Msg = Plain | TermOf(Ref)
  type Failure = NoFailure | StartFailure | MessageFailure
  type Directive = DRestart | DStop | DEscalate
  // The notice finishTerminate has still to send, after the first one.
  type Notice = NNone | NWatchers | NParent

  type Actor = {
    phase: Phase,
    inc: int,
    // An instance of the behaviour exists (Pekko's `actor != null`).
    inst: bool,
    handledCreate: bool,
    // The names this actor holds for its children (Pekko's ChildrenContainer).
    children: Set[str],
    toDie: Set[str],
    wait: Wait,
    sys: List[Sys],
    queue: List[Msg],
    suspend: int,
    // A failure the executor has not decided on yet.
    failure: Failure,
    postStopped: bool,
    pending: Notice,
  }

  type Ghost = {
    receivedWithoutInstance: bool,
    receivedWhileWaiting: bool,
    termBeforeChildTerm: bool,
    terminatedToAStoppingWatcher: bool,
    createdWithStaleChildren: bool,
    restartedWhileDying: bool,
    nested: bool,
    lostChildTerm: int,
    sysDead: int,
    deadLettered: int,
  }

  type Budget = { spawns: int, stops: int, fails: int, messages: int, restarts: int }

  type World = { actors: str -> Actor, rootChildren: Set[str], notices: Set[Ref], g: Ghost, b: Budget }

  var mode: Mode
  var w: World

  // ---- helpers -----------------------------------------------------------

  pure def fresh(inc: int): Actor = {
    phase: Free, inc: inc, inst: false, handledCreate: false, children: Set(), toDie: Set(), wait: WNone,
    sys: [], queue: [], suspend: 0, failure: NoFailure, postStopped: false, pending: NNone,
  }

  pure def act(x: World, n: str): Actor = x.actors.get(n)
  pure def updA(x: World, n: str, f: (Actor) => Actor): World = { ...x, actors: x.actors.set(n, f(x.actors.get(n))) }
  pure def isOpen(x: World, n: str): bool = Set(Unborn, Alive, StartFailed).contains(act(x, n).phase)
  pure def refOf(x: World, n: str): Ref = { name: n, inc: act(x, n).inc }
  pure def childrenOf(x: World, q: str): Set[str] = if (q == ROOT) x.rootChildren else act(x, q).children
  pure def setChildren(x: World, q: str, cs: Set[str]): World =
    if (q == ROOT) { ...x, rootChildren: cs } else updA(x, q, a => { ...a, children: cs })
  pure def less(n: int): int = if (n > 0) n - 1 else 0

  // systemEnqueue: a closed mailbox hands the message to dead letters.
  pure def sendSys(x: World, n: str, m: Sys): World =
    if (isOpen(x, n)) updA(x, n, a => { ...a, sys: a.sys.append(m) })
    else { ...x, g: { ...x.g, sysDead: x.g.sysDead + 1 } }

  pure def sendTerminateTo(x: World, targets: Set[str]): World = targets.fold(x, (acc, c) => sendSys(acc, c, Terminate))

  // The parent forgets the child (removeChildAndGetStateChange): the name is free.
  pure def releaseName(x: World, c: str): World = {
    val q = PARENT_OF.get(c)
    val x1 = setChildren(x, q, childrenOf(x, q).exclude(Set(c)))
    if (act(x1, c).phase == Closed) updA(x1, c, a => { ...a, phase: Free }) else x1
  }

  pure def tellParent(x: World, n: str): World = {
    val q = PARENT_OF.get(n)
    if (q == ROOT) releaseName(x, n)
    else if (isOpen(x, q)) updA(x, q, a => { ...a, sys: a.sys.append(ChildTerm(refOf(x, n))) })
    else { ...x, g: { ...x.g, lostChildTerm: x.g.lostChildTerm + 1 } }
  }

  pure def tellWatchers(md: Mode, x: World, n: str): World = {
    val x1 = { ...x, notices: x.notices.union(Set(refOf(x, n))) }
    val p = act(x1, WATCHER)
    if (n == WATCHED and isOpen(x1, WATCHER) and (p.wait != WTermination or md == KeepWatching))
      updA({ ...x1, g: { ...x1.g, terminatedToAStoppingWatcher: x1.g.terminatedToAStoppingWatcher or p.wait == WTermination } },
        WATCHER, a => { ...a, queue: a.queue.append(TermOf(refOf(x1, n))) })
    else x1
  }

  // finishTerminate: postStop (when there is an instance), close (what the
  // mailbox held goes to dead letters), then the parent and the watchers. The
  // second notice is a step of its own (tellRest), so another thread may run
  // the parent in between.
  pure def finishTerminate(md: Mode, x: World, n: str): World = {
    val a = act(x, n)
    val closed = {
      ...a, phase: Closed, inst: false, postStopped: a.inst, queue: [], sys: [], toDie: Set(), wait: WNone,
      pending: if (md == WatchersFirst) NParent else NWatchers,
    }
    val x1 = {
      ...updA(x, n, _ => closed),
      g: { ...x.g, deadLettered: x.g.deadLettered + a.queue.length(), sysDead: x.g.sysDead + a.sys.length() },
    }
    if (md == WatchersFirst) tellWatchers(md, x1, n) else tellParent(x1, n)
  }

  // finishCreate: the retried Create undoes the suspend of the failed start,
  // then builds the instance and runs preStart.
  pure def finishCreate(x: World, n: str, ok: bool): World = {
    val a = act(x, n)
    val resumed = if (a.phase == StartFailed) less(a.suspend) else a.suspend
    val stale = a.phase == StartFailed and a.children.size() > 0
    val x1 = { ...x, g: { ...x.g, createdWithStaleChildren: x.g.createdWithStaleChildren or stale } }
    val fails = not(ok) and x.b.fails > 0
    if (fails)
      updA({ ...x1, b: { ...x1.b, fails: x1.b.fails - 1 } }, n, y => {
        ...y, phase: StartFailed, inst: false, handledCreate: true, suspend: resumed + 1, failure: StartFailure, wait: WNone,
      })
    else updA(x1, n, y => { ...y, phase: Alive, inst: true, handledCreate: true, suspend: resumed, wait: WNone })
  }

  // finishRecreate: the new instance, postRestart, the resume of the failure's
  // suspend, then a Create for each child whose start failed and waits for
  // this restart.
  pure def finishRecreate(md: Mode, x: World, n: str): World = {
    val a = act(x, n)
    val x1 = { ...x, g: { ...x.g, restartedWhileDying: x.g.restartedWhileDying or a.toDie.size() > 0 } }
    val x2 = updA(x1, n, y => {
      ...y, inst: true, wait: WNone, toDie: Set(), suspend: if (md == ResumeOnRestart) y.suspend else less(y.suspend),
    })
    val waiting = a.children.filter(c => act(x2, c).phase == StartFailed and act(x2, c).failure == NoFailure)
    waiting.fold(x2, (acc, c) => sendSys(acc, c, Create))
  }

  pure def canRestart(x: World, n: str): bool = {
    val a = act(x, n)
    n != ROOT and a.phase == Alive and a.inst and a.wait == WNone and a.failure == NoFailure and x.b.restarts > 0
  }

  // The executor's restart (stage 4a): preRestart stops all or none of the
  // children; postRestart waits for them (Recreation). `suspendFirst` is for an
  // actor that did not fail itself (Pekko's suspendFirst, and the suspend of a
  // failing decider's actor).
  pure def restartActor(md: Mode, x: World, n: str, suspendFirst: bool, stopKids: bool): World = {
    val a = act(x, n)
    val stopping = if (stopKids) a.children else Set()
    val x1 = updA({ ...x, b: { ...x.b, restarts: x.b.restarts - 1 } }, n, y => {
      ...y, suspend: if (suspendFirst) y.suspend + 1 else y.suspend, toDie: y.toDie.union(stopping),
    })
    val x2 = sendTerminateTo(x1, stopping)
    val x3 = if (md == ResumeOnRestart) sendSys(x2, n, Resume) else x2
    if (act(x3, n).toDie.size() > 0 and md != NoRecreationWait) updA(x3, n, y => { ...y, wait: WRecreation })
    else finishRecreate(md, x3, n)
  }

  // Create: a retried start first stops the children of the failed attempt
  // and waits for them (faultCreate). A Create that reaches an actor that is
  // already stopping does nothing.
  pure def handleCreate(md: Mode, x: World, n: str, ok: bool): World = {
    val a = act(x, n)
    if (a.wait == WTermination or a.phase == Alive) x
    else if (a.phase == StartFailed and a.children.size() > 0 and md != NoCreationWait)
      updA(sendTerminateTo(x, a.children), n, y => { ...y, toDie: y.children, wait: WCreation })
    else finishCreate(x, n, ok)
  }

  // Terminate: unwatch, stop every child, and wait for them (suspended), or
  // finish at once when there is none.
  pure def handleTerminate(md: Mode, x: World, n: str): World = {
    val a = act(x, n)
    if (a.wait == WTermination) x
    else {
      val x0 = if (md == NotifyBeforePostStop) { ...x, notices: x.notices.union(Set(refOf(x, n))) } else x
      val x1 = if (md == EarlyNameRelease) releaseName(x0, n) else x0
      val x2 = if (md == NoTerminateToChildren) x1 else sendTerminateTo(x1, a.children)
      if (a.children.size() > 0 and md != CloseWithChildren)
        updA(x2, n, y => { ...y, toDie: y.children, wait: WTermination, suspend: y.suspend + 1 })
      else finishTerminate(md, x2, n)
    }
  }

  // A child's notice: the name is free, and the last of the children waited
  // for lets the actor finish what it waited for.
  pure def handleChildTerm(md: Mode, x: World, n: str, r: Ref, ok: bool): World = {
    val x1 = if (act(x, r.name).inc == r.inc) releaseName(x, r.name) else x
    val a = act(x1, n)
    val toDie = a.toDie.exclude(Set(r.name))
    val x2 = updA(x1, n, y => { ...y, toDie: toDie })
    if (a.wait != WNone and toDie.size() == 0)
      match a.wait {
        | WTermination => finishTerminate(md, x2, n)
        | WCreation => finishCreate(updA(x2, n, y => { ...y, wait: WNone }), n, ok)
        | WRecreation => finishRecreate(md, x2, n)
        | WNone => x2
      }
    else x2
  }

  // ---- actions -----------------------------------------------------------

  action setWorld(x: World): bool = all { w' = x, mode' = mode }

  // actorOf: refused when the name is taken or the parent is stopping. The
  // model also leaves out a spawn while the parent waits for other children,
  // and reuses a slot only once the run of its old incarnation has sent both
  // notices (a real name gets a new cell, so nothing of the old one is lost).
  action spawn(n: str): bool = {
    val q = PARENT_OF.get(n)
    all {
      w.b.spawns > 0,
      not(childrenOf(w, q).contains(n)),
      act(w, n).pending == NNone,
      q == ROOT or (isOpen(w, q) and act(w, q).wait == WNone),
      val born = { ...fresh(act(w, n).inc + 1), phase: Unborn, sys: [Create] }
      setWorld(setChildren({ ...updA(w, n, _ => born), b: { ...w.b, spawns: w.b.spawns - 1 } }, q,
        childrenOf(w, q).union(Set(n)))),
    }
  }

  action send(n: str): bool = all {
    w.b.messages > 0,
    act(w, n).phase != Free,
    val spent = { ...w, b: { ...w.b, messages: w.b.messages - 1 } }
    setWorld(
      if (isOpen(w, n)) updA(spent, n, a => { ...a, queue: a.queue.append(Plain) })
      else { ...spent, g: { ...spent.g, deadLettered: spent.g.deadLettered + 1 } }
    ),
  }

  // system.stop, context.stop: Terminate on the target's system queue. Under
  // SyncStop the executor stops an actor without children at once, outside its
  // run.
  action stop(n: str): bool = all {
    w.b.stops > 0,
    act(w, n).phase != Free,
    val spent = { ...w, b: { ...w.b, stops: w.b.stops - 1 } }
    setWorld(
      if (mode == SyncStop and isOpen(w, n) and act(w, n).children.size() == 0) finishTerminate(mode, spent, n)
      else sendSys(spent, n, Terminate)
    ),
  }

  // One system message of the actor's run.
  action runSys(n: str, ok: bool): bool = {
    val a = act(w, n)
    all {
      isOpen(w, n),
      a.sys.length() > 0,
      val x = updA(w, n, y => { ...y, sys: y.sys.tail() })
      setWorld(
        match a.sys.head() {
          | Create => handleCreate(mode, x, n, ok)
          | Terminate => handleTerminate(mode, x, n)
          | Resume => updA(x, n, y => { ...y, suspend: less(y.suspend) })
          | ChildTerm(r) => handleChildTerm(mode, x, n, r, ok)
        }
      ),
    }
  }

  // One normal message, which the mailbox hands over only when no system
  // message waits and the mailbox is not suspended. It may fail (under the
  // catcher): the actor suspends itself and waits for the decision.
  action receive(n: str, fails: bool): bool = {
    val a = act(w, n)
    all {
      isOpen(w, n),
      a.suspend == 0,
      a.sys.length() == 0,
      a.queue.length() > 0,
      val early = match a.queue.head() {
        | TermOf(r) => a.children.contains(r.name) and act(w, r.name).inc == r.inc
        | Plain => false
      }
      val failNow = fails and w.b.fails > 0 and a.inst
      val g1 = {
        ...w.g,
        receivedWithoutInstance: w.g.receivedWithoutInstance or not(a.inst),
        receivedWhileWaiting: w.g.receivedWhileWaiting or a.wait != WNone,
        termBeforeChildTerm: w.g.termBeforeChildTerm or early,
      }
      val x = { ...w, g: g1, b: if (failNow) { ...w.b, fails: w.b.fails - 1 } else w.b }
      setWorld(updA(x, n, y => {
        ...y,
        queue: y.queue.tail(),
        suspend: if (failNow) y.suspend + 1 else y.suspend,
        failure: if (failNow) MessageFailure else y.failure,
      })),
    }
  }

  // The executor's decision after the run that failed (stage 4a: still in the
  // executor; stage 4b makes it the parent's Failed). The actor may have
  // stopped since; the decision is still taken, and its effect on a stopped
  // actor is nothing.
  action decide(n: str, d: Directive, stopKids: bool, failAgain: bool): bool = {
    val a = act(w, n)
    val q = PARENT_OF.get(n)
    all {
      a.failure != NoFailure,
      d != DEscalate or canRestart(w, q),
      d != DRestart or a.failure == StartFailure or not(isOpen(w, n)) or (a.wait == WNone and w.b.restarts > 0),
      val x = updA(w, n, y => { ...y, failure: NoFailure })
      setWorld(
        match d {
          | DRestart =>
            if (a.failure == StartFailure) {
              val x1 = sendSys(x, n, Create)
              if (mode == ResumeAfterStartFailure) sendSys(x1, n, Resume) else x1
            } else if (isOpen(x, n)) restartActor(mode, x, n, false, stopKids)
            else x
          | DStop => sendSys(x, n, Terminate)
          | DEscalate => {
            val nested = mode == CreateInsideRestart and a.failure == StartFailure and failAgain
            val x1 = restartActor(mode, { ...x, g: { ...x.g, nested: x.g.nested or nested } }, q, true, stopKids)
            if (a.failure == MessageFailure) sendSys(x1, n, Resume) else x1
          }
        }
      ),
    }
  }

  // The second notice of finishTerminate.
  action tellRest(n: str): bool = {
    val a = act(w, n)
    all {
      a.pending != NNone,
      val x = updA(w, n, y => { ...y, pending: NNone })
      setWorld(if (a.pending == NWatchers) tellWatchers(mode, x, n) else tellParent(x, n)),
    }
  }

  // ---- invariants --------------------------------------------------------

  def phaseOf(n: str): Phase = act(w, n).phase

  // T1, T2: an actor closes, and so runs postStop, only once every child has
  // gone and the actor has handled its notice.
  val noCloseBeforeChildren = ACTORS.forall(n => not(isOpen(w, n)) implies act(w, n).children.size() == 0)
  // No child's notice reaches a closed parent.
  val noLostChildTerminated = w.g.lostChildTerm == 0
  // A live actor holds its name in its parent, and a held name has an actor.
  val namesHeldWhileLive = ACTORS.forall(n =>
    Set(Unborn, Alive, StartFailed).contains(phaseOf(n)) implies childrenOf(w, PARENT_OF.get(n)).contains(n))
  val heldNamesHaveActors = ACTORS.union(Set(ROOT)).forall(q =>
    (q == ROOT or phaseOf(q) != Free) implies childrenOf(w, q).forall(c => phaseOf(c) != Free))
  // T3, T4: a watcher learns of a stop only after postStop and the close.
  val noticesAfterTheEnd = w.notices.forall(r => act(w, r.name).inc > r.inc or Set(Closed, Free).contains(phaseOf(r.name)))
  // T4: a parent that also watches handles the child's end before its Terminated.
  val parentLearnsBeforeWatchers = not(w.g.termBeforeChildTerm)
  // T1: an actor that is stopping has stopped watching.
  val noTerminatedToAStoppingWatcher = not(w.g.terminatedToAStoppingWatcher)
  // Create is handled before Terminate: an actor closes only after its Create.
  val createBeforeTerminate = ACTORS.forall(n => phaseOf(n) == Closed implies act(w, n).handledCreate)
  // `receive` runs only on an instance, and never while the actor waits for children.
  val noReceiveWithoutInstance = not(w.g.receivedWithoutInstance)
  val noReceiveWhileWaiting = not(w.g.receivedWhileWaiting)
  // An actor that failed to start stays suspended until a retried Create.
  val failedStartStaysSuspended = ACTORS.forall(n => phaseOf(n) == StartFailed implies act(w, n).suspend >= 1)
  // A retried start builds the instance only after the attempt's children have gone.
  val retryWithoutTheAttemptsChildren = not(w.g.createdWithStaleChildren)
  // postRestart runs only after the children that preRestart stopped have gone.
  val restartAfterStoppedChildren = not(w.g.restartedWhileDying)
  // A restart never runs inside a restart of the same actor (depth 1, #251).
  val recoveryNotNested = not(w.g.nested)
  // When nothing is in progress, nobody waits for a child: every wait ends.
  val settled = ACTORS.forall(n => act(w, n).sys.length() == 0 and act(w, n).failure == NoFailure and act(w, n).pending == NNone)
  val waitsEnd = settled implies ACTORS.forall(n => act(w, n).wait == WNone)

  val invariant = noCloseBeforeChildren and noLostChildTerminated and namesHeldWhileLive and heldNamesHaveActors
    and noticesAfterTheEnd and parentLearnsBeforeWatchers and noTerminatedToAStoppingWatcher and createBeforeTerminate
    and noReceiveWithoutInstance and noReceiveWhileWaiting and failedStartStaysSuspended
    and retryWithoutTheAttemptsChildren and restartAfterStoppedChildren and recoveryNotNested and waitsEnd

  // ---- initial states and the step ---------------------------------------

  action initWith(md: Mode): bool = all {
    mode' = md,
    w' = {
      actors: ACTORS.mapBy(_ => fresh(0)),
      rootChildren: Set(),
      notices: Set(),
      g: {
        receivedWithoutInstance: false, receivedWhileWaiting: false, termBeforeChildTerm: false,
        terminatedToAStoppingWatcher: false, createdWithStaleChildren: false, restartedWhileDying: false,
        nested: false, lostChildTerm: 0, sysDead: 0, deadLettered: 0,
      },
      b: { spawns: SPAWNS, stops: STOPS, fails: FAILS, messages: MESSAGES, restarts: RESTARTS },
    },
  }

  action init = initWith(Fixed)

  action step = {
    nondet n = ACTORS.oneOf()
    nondet ok = Bool.oneOf()
    nondet fails = Bool.oneOf()
    nondet d = Set(DRestart, DStop, DEscalate).oneOf()
    nondet stopKids = Bool.oneOf()
    nondet failAgain = Bool.oneOf()
    any { spawn(n), send(n), stop(n), runSys(n, ok), receive(n, fails), decide(n, d, stopKids, failAgain), tellRest(n) }
  }

  // ---- runs: the fixed design -------------------------------------------

  // Create comes first on the system queue, so a stop asked for before the
  // actor has started runs preStart, then postStop.
  run aStopBehindTheCreateRunsAfterPreStartTest =
    initWith(Fixed).then(spawn("p")).then(stop("p")).then(runSys("p", true)).then(runSys("p", true))
      .then(tellRest("p"))
      .expect(phaseOf("p") == Free and act(w, "p").postStopped and w.notices.contains({ name: "p", inc: 1 }) and invariant)

  // T1 to T4: the parent stays open and suspended until its last child's
  // notice, and closes in the run that handles it.
  run aParentClosesInTheRunOfItsLastChildsNoticeTest =
    initWith(Fixed).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(stop("p")).then(runSys("p", true))
      .expect(act(w, "p").wait == WTermination and isOpen(w, "p") and act(w, "p").suspend == 1 and invariant)
      .then(runSys("c1", true)).then(tellRest("c1")).then(runSys("p", true))
      .expect(phaseOf("p") == Free and phaseOf("c1") == Free and act(w, "p").postStopped and invariant)

  // The name stays taken until the parent has handled the child's notice.
  run theNameStaysTakenUntilTheParentLearnsTest =
    initWith(Fixed).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(stop("c1")).then(runSys("c1", true))
      .expect(phaseOf("c1") == Closed and childrenOf(w, "p").contains("c1") and invariant)
      .then(tellRest("c1")).then(runSys("p", true)).then(spawn("c1"))
      .expect(act(w, "c1").inc == 2 and phaseOf("c1") == Unborn and invariant)

  // An actor that failed to start and is stopped runs no postStop.
  run aFailedStartThatIsStoppedRunsNoPostStopTest =
    initWith(Fixed).then(spawn("p")).then(runSys("p", false)).then(decide("p", DStop, false, false))
      .then(runSys("p", true))
      .expect(phaseOf("p") == Free and not(act(w, "p").postStopped) and invariant)

  // A retried start stops the children of the failed attempt and creates the
  // instance in the run that handles the last one's notice.
  run aRetriedStartStopsTheAttemptsChildrenFirstTest =
    initWith(Fixed).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", false))
      .then(spawn("g")).then(decide("c1", DRestart, false, false)).then(runSys("c1", true))
      .expect(act(w, "c1").wait == WCreation and phaseOf("c1") == StartFailed and act(w, "c1").suspend == 1)
      .then(runSys("g", true)).then(runSys("g", true)).then(runSys("c1", true))
      .expect(phaseOf("c1") == Alive and act(w, "c1").suspend == 0 and phaseOf("g") == Free and invariant)

  // A restart waits for the children its preRestart stopped, and is resumed
  // by finishRecreate.
  run aRestartWaitsForTheChildrenItStoppedTest =
    initWith(Fixed).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(send("p")).then(receive("p", true)).then(decide("p", DRestart, true, false))
      .expect(act(w, "p").wait == WRecreation and act(w, "p").suspend == 1 and invariant)
      .then(runSys("c1", true)).then(runSys("p", true))
      .expect(act(w, "p").wait == WNone and act(w, "p").suspend == 0 and phaseOf("c1") == Free and invariant)

  // A start failure whose decider fails restarts the parent, which retries the
  // child's start with a Create: the retry runs in the child's own run, after
  // the parent's restart has returned (depth 1).
  run anEscalatedStartFailureIsRetriedAfterTheParentRestartTest =
    initWith(Fixed).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", false))
      .then(decide("c1", DEscalate, false, true))
      .expect(act(w, "c1").sys == [Create] and act(w, "p").suspend == 0 and recoveryNotNested)
      .then(runSys("c1", true))
      .expect(phaseOf("c1") == Alive and act(w, "c1").suspend == 0 and invariant)

  // A parent that watches its child handles the child's notice before the
  // child's Terminated (T4).
  run aWatchingParentLearnsOfItsChildFirstTest =
    initWith(Fixed).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(stop("c1")).then(runSys("c1", true)).then(tellRest("c1")).then(runSys("p", true))
      .then(receive("p", false))
      .expect(parentLearnsBeforeWatchers and invariant)

  // ---- runs: the mistakes -------------------------------------------------

  run closeWithChildrenLosesTheChildsNoticeTest =
    initWith(CloseWithChildren).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(stop("p")).then(runSys("p", true))
      .expect(not(noCloseBeforeChildren))
      .then(runSys("c1", true))
      .expect(not(noLostChildTerminated))

  run notifyBeforePostStopTellsTheWatcherTooEarlyTest =
    initWith(NotifyBeforePostStop).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(stop("p")).then(runSys("p", true))
      .expect(not(noticesAfterTheEnd))

  run watchersFirstLetsTheParentSeeTerminatedFirstTest =
    initWith(WatchersFirst).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(stop("c1")).then(runSys("c1", true)).then(receive("p", false))
      .expect(not(parentLearnsBeforeWatchers))

  run earlyNameReleaseFreesTheNameOfALiveActorTest =
    initWith(EarlyNameRelease).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(spawn("g")).then(runSys("g", true)).then(stop("c1")).then(runSys("c1", true))
      .expect(not(namesHeldWhileLive))

  run syncStopClosesBeforeTheCreateTest =
    initWith(SyncStop).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(stop("c1"))
      .expect(not(createBeforeTerminate))

  run keepWatchingSendsTerminatedToAStoppingActorTest =
    initWith(KeepWatching).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(stop("p")).then(runSys("p", true)).then(runSys("c1", true)).then(tellRest("c1"))
      .expect(not(noTerminatedToAStoppingWatcher))

  run resumeAfterStartFailureWakesAnActorWithoutInstanceTest =
    initWith(ResumeAfterStartFailure).then(spawn("p")).then(runSys("p", false)).then(decide("p", DRestart, false, false))
      .then(runSys("p", false)).then(runSys("p", true))
      .expect(not(failedStartStaysSuspended))
      .then(send("p")).then(receive("p", false))
      .expect(not(noReceiveWithoutInstance))

  run resumeOnRestartReceivesWhileWaitingTest =
    initWith(ResumeOnRestart).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(send("p")).then(receive("p", true)).then(decide("p", DRestart, true, false)).then(runSys("p", true))
      .then(send("p")).then(receive("p", false))
      .expect(not(noReceiveWhileWaiting))

  run noCreationWaitKeepsTheAttemptsChildrenTest =
    initWith(NoCreationWait).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", false))
      .then(spawn("g")).then(decide("c1", DRestart, false, false)).then(runSys("c1", true))
      .expect(not(retryWithoutTheAttemptsChildren))

  run noRecreationWaitRestartsBeforeTheChildrenGoTest =
    initWith(NoRecreationWait).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(send("p")).then(receive("p", true)).then(decide("p", DRestart, true, false))
      .expect(not(restartAfterStoppedChildren))

  run createInsideRestartNestsTheRecoveryTest =
    initWith(CreateInsideRestart).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", false))
      .then(decide("c1", DEscalate, false, true))
      .expect(not(recoveryNotNested))

  run noTerminateToChildrenWaitsForeverTest =
    initWith(NoTerminateToChildren).then(spawn("p")).then(runSys("p", true)).then(spawn("c1")).then(runSys("c1", true))
      .then(stop("p")).then(runSys("p", true))
      .expect(not(waitsEnd))
}
