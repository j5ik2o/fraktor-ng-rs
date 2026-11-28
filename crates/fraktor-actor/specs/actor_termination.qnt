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

// The start, the stop and the supervision of actors in a tree, once all of
// them are system messages (stages 4a and 4b of #234:
// docs/design/issue-234-multithread-executor.md, sections 11.12 and 11.13).
// It mirrors Pekko's ActorCell: `actorOf` reserves the child's name in the
// parent and puts Create on the child's own system queue
// (dungeon/Children.makeChild, dungeon/Dispatch.init); Create builds the
// instance and runs preStart (ActorCell.create); Terminate stops the children
// and waits for them (ActorCell.terminate); the last child's notice lets the
// parent go on with what it waited for (FaultHandling.handleChildTerminated:
// finishCreate, finishRecreate, finishTerminate); finishTerminate runs
// postStop, closes the mailbox, tells the parent and then the watchers.
//
// Supervision (stage 4b) is Pekko's dungeon/FaultHandling: an actor that
// fails suspends itself and its children and puts Failed on its parent's
// system queue (handleInvokeFailure); the parent decides in its own run
// (handleFailure, OneForOneStrategy / AllForOneStrategy.processFailure):
// Recreate for a restart, Terminate for a stop, or its own failure when the
// decider fails (escalation). Suspend and Resume carry the recursive suspend
// (faultSuspend, faultResume); Recreate runs the restart (faultRecreate,
// finishRecreate, which recreates the children it keeps) or, without an
// instance, the retried start (faultCreate). A run keeps a system message
// aside while the actor is suspended (Failed) or waits for children
// (Recreate, Suspend, Resume, Failed), and takes them up again, before the
// rest of its queue, once the actor leaves that state (ActorCell.systemInvoke,
// shouldStash, StashWhenFailed, StashWhenWaitingForChildren).
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
// handles its children's notices and failures at once) holds p; p holds c1
// and c2; c1 holds g. A slot is one name, and a later incarnation reuses it
// once the name is free again. p watches c1, and an outside watcher watches
// every actor. p's strategy is one-for-one or all-for-one (`afo`, chosen at
// the start); the others are one-for-one.
//
// The modes other than Fixed are the mistakes the design avoids. Stage 4a:
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
//   NoCreationWait         : a retried start creates the instance while the
//                            children of the failed attempt are still there.
//   NoRecreationWait       : postRestart runs before the children that
//                            preRestart stopped have gone.
//   NoTerminateToChildren  : Terminate waits for children it never asked to
//                            stop.
//   NotifyTwice            : finishTerminate tells the parent and the watchers
//                            at once and then the watchers again.
//   KeepSuspended          : finishRecreate does not undo the suspend of the
//                            failure, so a restarted actor stays suspended.
// Stage 4b:
//   NoSuspendChildren      : a failure suspends only the actor that failed
//                            (stages 3 and 4a), so its children go on
//                            receiving before the decision.
//   NoStashWhenFailed      : a suspended parent decides a child's Failed at
//                            once, even without an instance.
//   NoStashWhileWaiting    : an actor that waits for children handles
//                            Recreate and the rest at once, so a second
//                            restart begins inside the one under way.
//   DropRecreateWhileWaiting: a Recreate that reaches an actor waiting for
//                            children is dropped (stage 4a skips that
//                            restart), while the Suspend before it is kept.
//   IgnoreUndecidedRecreate: a Recreate that reaches an actor whose failed
//                            start has not been decided yet does nothing
//                            (stage 4a's guard on the second Create).
//   NoSurvivorRecreate     : finishRecreate restarts none of the children it
//                            keeps (stage 4a does so only after a Kill).
//   NoChildCheck           : a parent decides a Failed without checking that
//                            the child is the incarnation it holds.
//   NoSuspendInheritance   : a child created under a suspended parent does
//                            not get the parent's suspend count as Suspends
//                            (Pekko's Children.makeChild does).
//   ResumeWithoutCause     : faultResume resumes every child without the
//                            cause, the perpetrator too, so the perpetrator
//                            keeps its failure (resumeChildren passes it on).
//   NoSuspendFirst         : all-for-one sends Recreate to the siblings that
//                            did not fail without a Suspend before it.
//   SuspendThePerpetrator  : handleInvokeFailure suspends every child, the
//                            one whose failure it escalates too (exceptFor).
//   StopAtTheTail          : a failed postRestart stops the actor with a
//                            Terminate at the tail of its queue, so what the
//                            run kept aside comes back before the Terminate.
//   HandleRecreateUnderCreation : a Recreate that reaches an actor waiting for
//                            children runs at once instead of being kept aside
//                            (an actor waiting in Creation included), so the
//                            retried start begins inside the one under way.
//   UntoldEscalation       : a top-level actor whose decider fails on a
//                            child's Failed tells nobody (#319: the
//                            `tell_parent = false` of c6a52e7), so it waits
//                            for an answer that never comes. Pekko tells the
//                            parent on every failure (handleInvokeFailure,
//                            dungeon/FaultHandling.scala:226-234).
//
// CI runs 20 steps (scripts/quint-check-specs.sh), which do not reach every
// deep state (a Recreate kept aside while an actor waits for its restart's
// children, a child created under a parent that waits); the runs below pin
// those states and the mistakes about them on every check. For deeper runs:
//   quint run --invariant=invariant --max-steps=40 --max-samples=50000 \
//     crates/fraktor-actor/specs/actor_termination.qnt
module actor_termination {
  pure val ROOT = "root"
  pure val ACTORS = Set("p", "c1", "c2", "g")
  pure val PARENT_OF = Map("p" -> ROOT, "c1" -> "p", "c2" -> "p", "g" -> "c1")
  // p watches c1.
  pure val WATCHER = "p"
  pure val WATCHED = "c1"
  // The actors above each actor, the guardian left out.
  pure val ANCESTORS = Map("p" -> Set(), "c1" -> Set("p"), "c2" -> Set("p"), "g" -> Set("c1", "p"))
  // The actor whose strategy may be all-for-one.
  pure val AFO_PARENT = "p"
  pure val SPAWNS = 6
  pure val STOPS = 2
  pure val FAILS = 3
  pure val MESSAGES = 2
  pure val RESTARTS = 3

  type Mode = Fixed | CloseWithChildren | NotifyBeforePostStop | WatchersFirst | EarlyNameRelease | SyncStop
    | KeepWatching | NoCreationWait | NoRecreationWait | NoTerminateToChildren | NotifyTwice | KeepSuspended
    | NoSuspendChildren | NoStashWhenFailed | NoStashWhileWaiting | DropRecreateWhileWaiting
    | IgnoreUndecidedRecreate | NoSurvivorRecreate | NoChildCheck | NoSuspendInheritance | ResumeWithoutCause
    | NoSuspendFirst | SuspendThePerpetrator | StopAtTheTail | HandleRecreateUnderCreation | UntoldEscalation

  // Free: no incarnation holds the name. Unborn: Create waits. StartFailed:
  // Create failed and the actor has no instance. Closed: finishTerminate ran;
  // the name stays taken until the parent has handled the child's notice.
  type Phase = Free | Unborn | Alive | StartFailed | Closed
  // What the actor waits for its children for (Pekko's SuspendReason).
  type Wait = WNone | WCreation | WRecreation | WTermination
  type Ref = { name: str, inc: int }
  // Pekko's Failed(child, cause, uid): `start` is a failed start
  // (ActorInitializationException).
  type Fail = { ref: Ref, start: bool }
  // Resume(true) is Pekko's Resume(causedByFailure) with a cause.
  type Sys = Create | Terminate | Resume(bool) | Suspend | Recreate | Failed(Fail) | ChildTerm(Ref)
  type Msg = Plain | TermOf(Ref)
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
    // The system messages a run kept aside (Pekko's sysMsgStash).
    stash: List[Sys],
    queue: List[Msg],
    suspend: int,
    // Pekko's _failed: "" when the actor has no failure its parent has not
    // answered yet; else the perpetrator (itself, or the child whose failure
    // it escalated).
    failed: str,
    // Stage 4a's mark that the parent decided a failed start; only
    // IgnoreUndecidedRecreate reads it.
    startDecided: bool,
    // How many times postStop ran for this incarnation.
    postStops: int,
    // How many restarts of this incarnation finished (postRestart ran).
    restarts: int,
    pending: Notice,
    // Restarts of this actor under way: from preRestart until postRestart
    // (the frame of the old behaviour lives that long).
    restarting: int,
    // postRestart failed (Pekko's FailedFatally): the actor only stops, and
    // its postStop does not run (preRestart already ran on the old instance).
    fatal: bool,
  }

  type Ghost = {
    receivedWithoutInstance: bool,
    receivedWhileWaiting: bool,
    termBeforeChildTerm: bool,
    terminatedToAStoppingWatcher: bool,
    createdWithStaleChildren: bool,
    restartedWhileDying: bool,
    // A child received a normal message while its parent had a failure that
    // its own parent had not answered yet.
    receivedUnderAFailedParent: bool,
    // A parent decided a child's Failed while it had no instance.
    decidedWithoutInstance: bool,
    // faultRecreate or faultCreate ran on an actor that was not suspended
    // (Pekko asserts it is).
    recreatedUnsuspended: bool,
    // An actor whose postRestart failed decided a Failed, ran a Recreate or
    // received a normal message.
    actedAfterAFailedRestart: bool,
    // The deepest restart seen: 1 plus the restarts of the same actor that
    // were under way when it began.
    deepestRestart: int,
    lostChildTerm: int,
    sysDead: int,
    deadLettered: int,
  }

  type Budget = { spawns: int, stops: int, fails: int, messages: int, restarts: int }

  // notices: the Terminated each incarnation's watchers were sent; parentNotices:
  // the ChildTerminated its parent was sent (or, under the guardian, the
  // release of its name). Both count, so a notice sent twice shows.
  type World = {
    actors: str -> Actor, rootChildren: Set[str], notices: Ref -> int, parentNotices: Ref -> int, afo: bool,
    g: Ghost, b: Budget,
  }

  var mode: Mode
  var w: World

  // ---- helpers -----------------------------------------------------------

  pure def fresh(inc: int): Actor = {
    phase: Free, inc: inc, inst: false, handledCreate: false, children: Set(), toDie: Set(), wait: WNone,
    sys: [], stash: [], queue: [], suspend: 0, failed: "", startDecided: false, postStops: 0, restarts: 0,
    pending: NNone, restarting: 0, fatal: false,
  }

  pure def act(x: World, n: str): Actor = x.actors.get(n)
  pure def updA(x: World, n: str, f: (Actor) => Actor): World = { ...x, actors: x.actors.set(n, f(x.actors.get(n))) }
  pure def isOpen(x: World, n: str): bool = Set(Unborn, Alive, StartFailed).contains(act(x, n).phase)
  pure def refOf(x: World, n: str): Ref = { name: n, inc: act(x, n).inc }
  pure def childrenOf(x: World, q: str): Set[str] = if (q == ROOT) x.rootChildren else act(x, q).children
  pure def setChildren(x: World, q: str, cs: Set[str]): World =
    if (q == ROOT) { ...x, rootChildren: cs } else updA(x, q, a => { ...a, children: cs })
  pure def less(n: int): int = if (n > 0) n - 1 else 0
  pure def max(a: int, b: int): int = if (a > b) a else b
  pure def bump(m: Ref -> int, r: Ref): Ref -> int = m.put(r, if (m.keys().contains(r)) m.get(r) + 1 else 1)
  pure def isAllForOne(x: World, n: str): bool = n == AFO_PARENT and x.afo

  // Pekko's systemInvoke states: 0 DefaultState, 1 SuspendedState,
  // 2 SuspendedWaitForChildrenState.
  pure def stateOf(a: Actor): int = if (a.wait != WNone) 2 else if (a.suspend > 0) 1 else 0

  pure def isFailed(m: Sys): bool = match m {
    | Failed(_) => true
    | _ => false
  }

  // StashWhenWaitingForChildren: Recreate, Suspend, Resume and Failed.
  pure def waitsForChildren(m: Sys): bool = match m {
    | Recreate => true
    | Suspend => true
    | Resume(_) => true
    | Failed(_) => true
    | _ => false
  }

  pure def shouldStash(md: Mode, m: Sys, state: int): bool =
    if (state == 1) md != NoStashWhenFailed and isFailed(m)
    else if (state == 2)
      md != NoStashWhileWaiting and waitsForChildren(m)
        and not(md == HandleRecreateUnderCreation and m == Recreate)
    else false

  // systemEnqueue: a closed mailbox hands the message to dead letters.
  pure def sendSys(x: World, n: str, m: Sys): World =
    if (isOpen(x, n)) updA(x, n, a => { ...a, sys: a.sys.append(m) })
    else { ...x, g: { ...x.g, sysDead: x.g.sysDead + 1 } }

  pure def sendTo(x: World, targets: Set[str], m: Sys): World = targets.fold(x, (acc, c) => sendSys(acc, c, m))
  pure def sendTerminateTo(x: World, targets: Set[str]): World = sendTo(x, targets, Terminate)

  // The parent forgets the child (removeChildAndGetStateChange): the name is free.
  pure def releaseName(x: World, c: str): World = {
    val q = PARENT_OF.get(c)
    val x1 = setChildren(x, q, childrenOf(x, q).exclude(Set(c)))
    if (act(x1, c).phase == Closed) updA(x1, c, a => { ...a, phase: Free }) else x1
  }

  pure def tellParent(x: World, n: str): World = {
    val q = PARENT_OF.get(n)
    val x0 = { ...x, parentNotices: bump(x.parentNotices, refOf(x, n)) }
    if (q == ROOT) releaseName(x0, n)
    else if (isOpen(x0, q)) updA(x0, q, a => { ...a, sys: a.sys.append(ChildTerm(refOf(x0, n))) })
    else { ...x0, g: { ...x0.g, lostChildTerm: x0.g.lostChildTerm + 1 } }
  }

  pure def tellWatchers(md: Mode, x: World, n: str): World = {
    val x1 = { ...x, notices: bump(x.notices, refOf(x, n)) }
    val p = act(x1, WATCHER)
    if (n == WATCHED and isOpen(x1, WATCHER) and (p.wait != WTermination or md == KeepWatching))
      updA({ ...x1, g: { ...x1.g, terminatedToAStoppingWatcher: x1.g.terminatedToAStoppingWatcher or p.wait == WTermination } },
        WATCHER, a => { ...a, queue: a.queue.append(TermOf(refOf(x1, n))) })
    else x1
  }

  // finishTerminate: postStop (when there is an instance), close (what the
  // mailbox held, and what the run kept aside, goes to dead letters), then
  // the parent and the watchers. The second notice is a step of its own
  // (tellRest), so another thread may run the parent in between.
  pure def finishTerminate(md: Mode, x: World, n: str): World = {
    val a = act(x, n)
    val closed = {
      ...a, phase: Closed, inst: false, postStops: a.postStops + (if (a.inst and not(a.fatal)) 1 else 0), queue: [],
      sys: [], stash: [],
      toDie: Set(), wait: WNone, failed: "",
      pending: if (md == WatchersFirst) NParent else NWatchers,
    }
    val x1 = {
      ...updA(x, n, _ => closed),
      g: {
        ...x.g,
        deadLettered: x.g.deadLettered + a.queue.length(),
        sysDead: x.g.sysDead + a.sys.length() + a.stash.length(),
      },
    }
    if (md == WatchersFirst) tellWatchers(md, x1, n)
    else if (md == NotifyTwice) tellWatchers(md, tellParent(x1, n), n)
    else tellParent(x1, n)
  }

  // The executor decides at once for a child of the guardian, which has no
  // mailbox: Restart (while the budget lasts) or Stop. A failing decider of
  // the guardian resumes a child that has an instance, as stage 4a does, and
  // leaves a child whose start failed as it is.
  pure def rootDecide(x: World, n: str, start: bool, rd: Directive): World = match rd {
    | DRestart =>
      if (x.b.restarts > 0) sendSys({ ...x, b: { ...x.b, restarts: x.b.restarts - 1 } }, n, Recreate)
      else sendSys(x, n, Terminate)
    | DStop => sendSys(x, n, Terminate)
    | DEscalate => if (start) x else sendSys(x, n, Resume(true))
  }

  // handleInvokeFailure: suspend the actor and its children (all but the
  // child whose failure this escalates), remember the perpetrator unless a
  // failure is already outstanding, and tell the parent.
  pure def failActor(md: Mode, x: World, n: str, start: bool, perp: str, rd: Directive): World = {
    val a = act(x, n)
    val first = a.failed == ""
    val skip = if (first and perp != n and md != SuspendThePerpetrator) Set(perp) else Set()
    val x1 = updA(x, n, y => { ...y, suspend: y.suspend + 1, failed: if (first) perp else y.failed })
    val x2 = if (md == NoSuspendChildren) x1 else sendTo(x1, a.children.exclude(skip), Suspend)
    val q = PARENT_OF.get(n)
    if (q == ROOT) (if (md == UntoldEscalation and perp != n) x2 else rootDecide(x2, n, start, rd))
    else sendSys(x2, q, Failed({ ref: refOf(x2, n), start: start }))
  }

  // ActorCell.create: build the instance and run preStart; a failure is
  // handleInvokeFailure on the actor itself.
  pure def create(md: Mode, x: World, n: str, ok: bool, rd: Directive): World =
    if (not(ok) and x.b.fails > 0)
      failActor(md, updA({ ...x, b: { ...x.b, fails: x.b.fails - 1 } }, n, y => {
        ...y, phase: StartFailed, inst: false, handledCreate: true,
      }), n, true, n, rd)
    else updA(x, n, y => { ...y, phase: Alive, inst: true, handledCreate: true })

  // finishCreate: undo the suspend of the failed start, clear the failure,
  // and create again.
  pure def finishCreate(md: Mode, x: World, n: str, ok: bool, rd: Directive): World = {
    val a = act(x, n)
    val x1 = { ...x, g: { ...x.g, createdWithStaleChildren: x.g.createdWithStaleChildren or a.children.size() > 0 } }
    create(md, updA(x1, n, y => { ...y, suspend: less(y.suspend), failed: "", wait: WNone, startDecided: false }), n, ok, rd)
  }

  // faultCreate: stop the children of the failed attempt and wait for them
  // (Creation), or create again at once.
  pure def faultCreate(md: Mode, x: World, n: str, ok: bool, rd: Directive): World = {
    val a = act(x, n)
    val x1 = { ...x, g: { ...x.g, recreatedUnsuspended: x.g.recreatedUnsuspended or a.suspend == 0 } }
    if (a.children.size() > 0 and md != NoCreationWait)
      updA(sendTerminateTo(x1, a.children), n, y => { ...y, toDie: y.children, wait: WCreation })
    else finishCreate(md, x1, n, ok, rd)
  }

  // finishRecreate: the resume of the suspend, the new instance and
  // postRestart, the end of the restart, then a Recreate for every child it
  // keeps (the survivors). `ok` is the outcome of postRestart. A failed one
  // stops the actor (stage 4b keeps stage 4a's stop; #301 makes it the
  // parent's decision): the run handles the stop at once, as Terminate, so
  // that the actor waits for its children (state 2) or closes before the
  // state can go down and give back what the run kept aside. The survivors
  // get the Terminate instead of a Recreate.
  pure def finishRecreate(md: Mode, x: World, n: str, ok: bool): World = {
    val a = act(x, n)
    val x1 = { ...x, g: { ...x.g, restartedWhileDying: x.g.restartedWhileDying or a.toDie.size() > 0 } }
    val keep = md == KeepSuspended
    val x2 = updA(x1, n, y => {
      ...y, inst: true, wait: WNone, toDie: Set(), suspend: if (keep) y.suspend else less(y.suspend), failed: "",
      restarting: less(y.restarting),
    })
    if (not(ok) and x2.b.fails > 0) {
      val x3 = updA({ ...x2, b: { ...x2.b, fails: x2.b.fails - 1 } }, n, y => { ...y, fatal: true })
      if (md == StopAtTheTail) sendSys(x3, n, Terminate) else handleTerminate(md, x3, n)
    } else {
      val x3 = updA(x2, n, y => { ...y, restarts: y.restarts + 1 })
      if (md == NoSurvivorRecreate) x3 else sendTo(x3, a.children, Recreate)
    }
  }

  // The first half of faultRecreate: preRestart stops all or none of the
  // children; postRestart waits for the dying ones (Recreation).
  pure def restartActor(md: Mode, x: World, n: str, stopKids: bool, ok: bool): World = {
    val a = act(x, n)
    val stopping = if (stopKids) a.children else Set()
    val x0 = { ...x, g: { ...x.g, deepestRestart: max(x.g.deepestRestart, a.restarting + 1) } }
    val x1 = updA(x0, n, y => { ...y, toDie: y.toDie.union(stopping), restarting: y.restarting + 1 })
    val x2 = sendTerminateTo(x1, stopping)
    if (act(x2, n).toDie.size() > 0 and md != NoRecreationWait) updA(x2, n, y => { ...y, wait: WRecreation })
    else finishRecreate(md, x2, n, ok)
  }

  // faultResume: without an instance, the retried start (Pekko turns the
  // Resume into a Create); else undo one suspend, clear the failure when the
  // Resume answers it, and resume the children, the perpetrator with the
  // cause (resumeChildren).
  pure def faultResume(md: Mode, x: World, n: str, byFailure: bool, ok: bool, rd: Directive): World = {
    val a = act(x, n)
    if (not(a.inst)) faultCreate(md, x, n, ok, rd)
    else {
      val x1 = updA(x, n, y => { ...y, suspend: less(y.suspend), failed: if (byFailure) "" else y.failed })
      if (md == NoSuspendChildren) x1
      else a.children.fold(x1, (acc, c) => sendSys(acc, c, Resume(byFailure and c == a.failed and md != ResumeWithoutCause)))
    }
  }

  // faultSuspend: suspend the actor and its children.
  pure def faultSuspend(md: Mode, x: World, n: str): World = {
    val a = act(x, n)
    val x1 = updA(x, n, y => { ...y, suspend: y.suspend + 1 })
    if (md == NoSuspendChildren) x1 else sendTo(x1, a.children, Suspend)
  }

  // faultRecreate: a restart, or the retried start when there is no
  // instance. An actor that waits for children (Creation, Recreation or
  // Termination) keeps the Recreate aside and only gets here under
  // NoStashWhileWaiting, so Pekko's branch for an actor that is not normal
  // (faultResume, reached in Pekko while it waits in Termination) has no
  // counterpart here.
  pure def faultRecreate(md: Mode, x: World, n: str, stopKids: bool, ok: bool, rd: Directive): World = {
    val a = act(x, n)
    val x1 = { ...x, g: { ...x.g, recreatedUnsuspended: x.g.recreatedUnsuspended or a.suspend == 0 } }
    if (md == IgnoreUndecidedRecreate and a.phase == StartFailed and not(a.startDecided)) x
    else if (not(a.inst)) faultCreate(md, x, n, ok, rd)
    else restartActor(md, x1, n, stopKids, ok)
  }

  // handleFailure in the parent's run: only a child the parent holds, by the
  // incarnation the Failed names, is decided for. Restart (while the budget
  // lasts) sends Recreate, to every child under all-for-one with a Suspend
  // first for those that did not fail; Stop sends Terminate; a failing
  // decider fails the parent (escalation), with the child as perpetrator.
  pure def handleFailed(md: Mode, x: World, n: str, f: Fail, d: Directive, rd: Directive): World = {
    val a = act(x, n)
    val c = f.ref.name
    val x0 = { ...x, g: { ...x.g, decidedWithoutInstance: x.g.decidedWithoutInstance or not(a.inst) } }
    val known = a.children.contains(c) and (md == NoChildCheck or act(x0, c).inc == f.ref.inc)
    val stopAll = if (isAllForOne(x0, n)) sendTerminateTo(x0, a.children) else sendSys(x0, c, Terminate)
    if (not(known)) x0
    else match d {
      | DRestart =>
        if (x0.b.restarts > 0) {
          val x1 = updA({ ...x0, b: { ...x0.b, restarts: x0.b.restarts - 1 } }, c, y => { ...y, startDecided: f.start })
          if (isAllForOne(x1, n))
            a.children.fold(x1, (acc, s) =>
              sendSys(if (s == c or md == NoSuspendFirst) acc else sendSys(acc, s, Suspend), s, Recreate))
          else sendSys(x1, c, Recreate)
        } else stopAll
      | DStop => stopAll
      | DEscalate => failActor(md, { ...x0, b: { ...x0.b, fails: x0.b.fails - 1 } }, n, false, c, rd)
    }
  }

  // The first Create (the only one in stage 4b: a retried start is a
  // Recreate). An actor that already stops does nothing.
  pure def handleCreate(md: Mode, x: World, n: str, ok: bool, rd: Directive): World = {
    val a = act(x, n)
    if (a.wait == WTermination or a.handledCreate) x else create(md, x, n, ok, rd)
  }

  // Terminate: unwatch, stop every child, and wait for them (suspended), or
  // finish at once when there is none.
  pure def handleTerminate(md: Mode, x: World, n: str): World = {
    val a = act(x, n)
    if (a.wait == WTermination) x
    else {
      val x0 = if (md == NotifyBeforePostStop) { ...x, notices: bump(x.notices, refOf(x, n)) } else x
      val x1 = if (md == EarlyNameRelease) releaseName(x0, n) else x0
      val x2 = if (md == NoTerminateToChildren) x1 else sendTerminateTo(x1, a.children)
      if (a.children.size() > 0 and md != CloseWithChildren)
        updA(x2, n, y => { ...y, toDie: y.children, wait: WTermination, suspend: y.suspend + 1 })
      else finishTerminate(md, x2, n)
    }
  }

  // A child's notice: the name is free, and the last of the children waited
  // for lets the actor finish what it waited for.
  pure def handleChildTerm(md: Mode, x: World, n: str, r: Ref, ok: bool, rd: Directive): World = {
    val x1 = if (act(x, r.name).inc == r.inc) releaseName(x, r.name) else x
    val a = act(x1, n)
    val toDie = a.toDie.exclude(Set(r.name))
    val x2 = updA(x1, n, y => { ...y, toDie: toDie })
    if (a.wait != WNone and toDie.size() == 0)
      match a.wait {
        | WTermination => finishTerminate(md, x2, n)
        | WCreation => finishCreate(md, x2, n, ok, rd)
        | WRecreation => finishRecreate(md, x2, n, ok)
        | WNone => x2
      }
    else x2
  }

  pure def handleSys(md: Mode, x0: World, n: str, m: Sys, ok: bool, d: Directive, rd: Directive, stopKids: bool): World = {
    val acts = match m {
      | Recreate => true
      | Failed(_) => true
      | _ => false
    }
    val x = { ...x0, g: { ...x0.g, actedAfterAFailedRestart: x0.g.actedAfterAFailedRestart or (acts and act(x0, n).fatal) } }
    match m {
      | Create => handleCreate(md, x, n, ok, rd)
      | Terminate => handleTerminate(md, x, n)
      | Resume(byFailure) => faultResume(md, x, n, byFailure, ok, rd)
      | Suspend => faultSuspend(md, x, n)
      | Recreate => faultRecreate(md, x, n, stopKids, ok, rd)
      | Failed(f) => handleFailed(md, x, n, f, d, rd)
      | ChildTerm(r) => handleChildTerm(md, x, n, r, ok, rd)
    }
  }

  // Once the actor's state goes down (Pekko: newState < currentState), what
  // the run kept aside comes back before the rest of the queue.
  pure def unstashIfDown(x: World, n: str, before: int): World = {
    val a = act(x, n)
    if (isOpen(x, n) and stateOf(a) < before and a.stash.length() > 0)
      updA(x, n, y => { ...y, sys: y.stash.concat(y.sys), stash: [] })
    else x
  }

  // ---- actions -----------------------------------------------------------

  action setWorld(x: World): bool = all { w' = x, mode' = mode }

  // actorOf: refused when the name is taken or the parent is stopping. The
  // child gets Create and then one Suspend for each suspend of its parent's
  // mailbox (Pekko's Children.makeChild), so a child created under a
  // suspended parent (from a hook such as handleChildTerminated or
  // preRestart, or from outside) is suspended like the children the
  // parent's failure or Suspend reached. The model leaves out a spawn while
  // the parent waits in Creation (it has no instance, so only an outside
  // spawn could), and reuses a slot only once the run of its old incarnation
  // has sent both notices (a real name gets a new cell, so nothing of the old
  // one is lost).
  action spawn(n: str): bool = {
    val q = PARENT_OF.get(n)
    val carried = if (q == ROOT or mode == NoSuspendInheritance) 0 else act(w, q).suspend
    all {
      w.b.spawns > 0,
      not(childrenOf(w, q).contains(n)),
      act(w, n).pending == NNone,
      q == ROOT or (isOpen(w, q) and Set(WNone, WRecreation).contains(act(w, q).wait)),
      val born = {
        ...fresh(act(w, n).inc + 1), phase: Unborn, sys: range(0, carried).foldl([Create], (acc, _) => acc.append(Suspend)),
      }
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

  // One system message of the actor's run, kept aside when the actor's state
  // says so. `ok` is the outcome of a creation this step runs, `d` the
  // decision on a Failed, `rd` the guardian's decision on a failure this step
  // reports to it, and `stopKids` what a preRestart this step runs does.
  action runSys(n: str, ok: bool, d: Directive, rd: Directive, stopKids: bool): bool = {
    val a = act(w, n)
    all {
      isOpen(w, n),
      a.sys.length() > 0,
      d != DEscalate or w.b.fails > 0,
      val m = a.sys.head()
      val st = stateOf(a)
      val x = updA(w, n, y => { ...y, sys: y.sys.tail() })
      val handled =
        if (mode == DropRecreateWhileWaiting and st == 2 and m == Recreate) x
        else if (shouldStash(mode, m, st)) updA(x, n, y => { ...y, stash: y.stash.append(m) })
        else handleSys(mode, x, n, m, ok, d, rd, stopKids)
      setWorld(unstashIfDown(handled, n, st)),
    }
  }

  // A run step with the defaults: a creation succeeds or fails by `ok`, a
  // Failed is decided Restart, and preRestart keeps the children.
  action sysStep(n: str, ok: bool): bool = runSys(n, ok, DRestart, DRestart, false)

  // One normal message, which the mailbox hands over only when no system
  // message waits and the mailbox is not suspended. It may fail (under the
  // catcher): handleInvokeFailure.
  action receive(n: str, fails: bool, rd: Directive): bool = {
    val a = act(w, n)
    val q = PARENT_OF.get(n)
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
        actedAfterAFailedRestart: w.g.actedAfterAFailedRestart or a.fatal,
        receivedUnderAFailedParent: w.g.receivedUnderAFailedParent or (q != ROOT and act(w, q).failed != ""),
      }
      val x = updA({ ...w, g: g1, b: if (failNow) { ...w.b, fails: w.b.fails - 1 } else w.b }, n, y => {
        ...y, queue: y.queue.tail(),
      })
      setWorld(if (failNow) failActor(mode, x, n, false, n, rd) else x),
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
  val noticesAfterTheEnd = w.notices.keys().forall(r => act(w, r.name).inc > r.inc or Set(Closed, Free).contains(phaseOf(r.name)))
  // T4: each incarnation's end is told once to its watchers and once to its
  // parent, and its postStop runs at most once.
  val noticesOnce = w.notices.keys().forall(r => w.notices.get(r) <= 1)
    and w.parentNotices.keys().forall(r => w.parentNotices.get(r) <= 1)
  val postStopOnce = ACTORS.forall(n => act(w, n).postStops <= 1)
  // T4: a parent that also watches handles the child's end before its Terminated.
  val parentLearnsBeforeWatchers = not(w.g.termBeforeChildTerm)
  // T1: an actor that is stopping has stopped watching.
  val noTerminatedToAStoppingWatcher = not(w.g.terminatedToAStoppingWatcher)
  // Create is handled before Terminate: an actor closes only after its Create.
  val createBeforeTerminate = ACTORS.forall(n => phaseOf(n) == Closed implies act(w, n).handledCreate)
  // `receive` runs only on an instance, and never while the actor waits for children.
  val noReceiveWithoutInstance = not(w.g.receivedWithoutInstance)
  val noReceiveWhileWaiting = not(w.g.receivedWhileWaiting)
  // An actor that failed to start stays suspended until a retried start.
  val failedStartStaysSuspended = ACTORS.forall(n => phaseOf(n) == StartFailed implies act(w, n).suspend >= 1)
  // A retried start builds the instance only after the attempt's children have gone.
  val retryWithoutTheAttemptsChildren = not(w.g.createdWithStaleChildren)
  // postRestart runs only after the children that preRestart stopped have gone.
  val restartAfterStoppedChildren = not(w.g.restartedWhileDying)
  // A restart never begins while a restart of the same actor is under way
  // (depth 1, #251).
  val recoveryNotNested = w.g.deepestRestart <= 1
  // Stage 4b: the children of an actor whose failure is not answered yet do
  // not receive (the recursive suspend of handleInvokeFailure).
  val noReceiveUnderAFailedParent = not(w.g.receivedUnderAFailedParent)
  // Stage 4b: a parent decides only with an instance (its strategy), so a
  // Failed waits while the parent is suspended.
  val decideWithInstance = not(w.g.decidedWithoutInstance)
  // Stage 4b: every Recreate (and every retried start) reaches an actor that
  // is suspended, so the resume of finishRecreate and finishCreate answers a
  // suspend (Pekko's asserts in faultRecreate and faultCreate).
  val recreateWhileSuspended = not(w.g.recreatedUnsuspended)
  // Stage 4b: a run keeps system messages aside only while the actor is
  // suspended or waits for children.
  // Stage 4b: an actor whose postRestart failed only stops: what its run kept
  // aside, and what comes after, is not run before it closes.
  // This is stage 4b's provisional stop, not Pekko's property: Pekko hands the
  // failure to the parent (FaultHandling.scala:298-302); the state goes from 2
  // to 1 with the children container back to normal
  // (ChildrenContainer.scala:181-186, ActorCell.scala:545-548), so a kept
  // Recreate comes back and builds a third instance. #301 makes it Pekko's.
  val aFailedRestartOnlyStops = not(w.g.actedAfterAFailedRestart)
  val stashOnlyWhileSuspended = ACTORS.forall(n => isOpen(w, n) and act(w, n).stash.length() > 0 implies stateOf(act(w, n)) > 0)
  // When nothing is in progress, nobody waits for a child: every wait ends.
  val settled = ACTORS.forall(n => act(w, n).sys.length() == 0 and act(w, n).pending == NNone)
  val waitsEnd = settled implies ACTORS.forall(n => act(w, n).wait == WNone)
  // When nothing is in progress, no actor with an instance (or about to get
  // one) is left suspended or holds a system message aside: every suspend
  // was answered by one resume (one suspend per failure and per Suspend, one
  // resume per decision's effect and per Recreate or Resume).
  //
  // Left out: the actors below one whose failed start nobody answers. That
  // is a child of the guardian whose decider failed on its failed start (the
  // guardian leaves it as it is, as in stage 4a; section 11.13.4); its
  // failure suspended its children, and they stay suspended with it.
  def underAnAbandonedStart(n: str): bool = ANCESTORS.get(n).exists(q => phaseOf(q) == StartFailed)
  val settledMeansResumed = settled implies ACTORS.forall(n =>
    (Set(Unborn, Alive).contains(phaseOf(n)) and not(underAnAbandonedStart(n)))
      implies (act(w, n).suspend == 0 and act(w, n).stash.length() == 0))

  val invariant = noCloseBeforeChildren and noLostChildTerminated and namesHeldWhileLive and heldNamesHaveActors
    and noticesAfterTheEnd and noticesOnce and postStopOnce and parentLearnsBeforeWatchers
    and noTerminatedToAStoppingWatcher and createBeforeTerminate and noReceiveWithoutInstance and noReceiveWhileWaiting
    and failedStartStaysSuspended and retryWithoutTheAttemptsChildren and restartAfterStoppedChildren
    and recoveryNotNested and noReceiveUnderAFailedParent and decideWithInstance and recreateWhileSuspended
    and stashOnlyWhileSuspended and aFailedRestartOnlyStops and waitsEnd and settledMeansResumed

  // ---- initial states and the step ---------------------------------------

  action initWith(md: Mode, afo: bool): bool = all {
    mode' = md,
    w' = {
      actors: ACTORS.mapBy(_ => fresh(0)),
      rootChildren: Set(),
      notices: Map(),
      parentNotices: Map(),
      afo: afo,
      g: {
        receivedWithoutInstance: false, receivedWhileWaiting: false, termBeforeChildTerm: false,
        terminatedToAStoppingWatcher: false, createdWithStaleChildren: false, restartedWhileDying: false,
        receivedUnderAFailedParent: false, decidedWithoutInstance: false, recreatedUnsuspended: false,
        actedAfterAFailedRestart: false,
        deepestRestart: 0, lostChildTerm: 0, sysDead: 0, deadLettered: 0,
      },
      b: { spawns: SPAWNS, stops: STOPS, fails: FAILS, messages: MESSAGES, restarts: RESTARTS },
    },
  }

  action init = {
    nondet afo = Bool.oneOf()
    initWith(Fixed, afo)
  }

  action step = {
    nondet n = ACTORS.oneOf()
    nondet ok = Bool.oneOf()
    nondet fails = Bool.oneOf()
    nondet d = Set(DRestart, DStop, DEscalate).oneOf()
    nondet rd = Set(DRestart, DStop, DEscalate).oneOf()
    nondet stopKids = Bool.oneOf()
    any { spawn(n), send(n), stop(n), runSys(n, ok, d, rd, stopKids), receive(n, fails, rd), tellRest(n) }
  }

  // ---- runs: the fixed design, stage 4a ------------------------------------

  // Create comes first on the system queue, so a stop asked for before the
  // actor has started runs preStart, then postStop.
  run aStopBehindTheCreateRunsAfterPreStartTest =
    initWith(Fixed, false).then(spawn("p")).then(stop("p")).then(sysStep("p", true)).then(sysStep("p", true))
      .then(tellRest("p"))
      .expect(phaseOf("p") == Free and act(w, "p").postStops == 1 and w.notices.get({ name: "p", inc: 1 }) == 1 and invariant)

  // T1 to T4: the parent stays open and suspended until its last child's
  // notice, and closes in the run that handles it.
  run aParentClosesInTheRunOfItsLastChildsNoticeTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(stop("p")).then(sysStep("p", true))
      .expect(act(w, "p").wait == WTermination and isOpen(w, "p") and act(w, "p").suspend == 1 and invariant)
      .then(sysStep("c1", true)).then(tellRest("c1")).then(sysStep("p", true))
      .expect(phaseOf("p") == Free and phaseOf("c1") == Free and act(w, "p").postStops == 1 and invariant)

  // The name stays taken until the parent has handled the child's notice.
  run theNameStaysTakenUntilTheParentLearnsTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(stop("c1")).then(sysStep("c1", true))
      .expect(phaseOf("c1") == Closed and childrenOf(w, "p").contains("c1") and invariant)
      .then(tellRest("c1")).then(sysStep("p", true)).then(spawn("c1"))
      .expect(act(w, "c1").inc == 2 and phaseOf("c1") == Unborn and invariant)

  // An actor that failed to start and is stopped runs no postStop. The
  // guardian decides at once (Stop).
  run aFailedStartThatIsStoppedRunsNoPostStopTest =
    initWith(Fixed, false).then(spawn("p")).then(runSys("p", false, DRestart, DStop, false))
      .expect(act(w, "p").sys == [Terminate] and phaseOf("p") == StartFailed and invariant)
      .then(sysStep("p", true))
      .expect(phaseOf("p") == Free and act(w, "p").postStops == 0 and invariant)

  // A retried start stops the children of the failed attempt and creates the
  // instance in the run that handles the last one's notice.
  run aRetriedStartStopsTheAttemptsChildrenFirstTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(spawn("g"))
      .then(sysStep("c1", false)).then(sysStep("g", true)).then(sysStep("g", true))
      .expect(act(w, "g").suspend == 1 and act(w, "p").sys.length() == 1 and invariant)
      .then(sysStep("p", true)).then(sysStep("c1", true))
      .expect(act(w, "c1").wait == WCreation and phaseOf("c1") == StartFailed and act(w, "c1").suspend == 1)
      .then(sysStep("g", true)).then(sysStep("c1", true))
      .expect(phaseOf("c1") == Alive and act(w, "c1").suspend == 0 and phaseOf("g") == Free and invariant)

  // A restart waits for the children its preRestart stopped, and is resumed
  // by finishRecreate.
  run aRestartWaitsForTheChildrenItStoppedTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(send("p")).then(receive("p", true, DRestart)).then(runSys("p", true, DRestart, DRestart, true))
      .expect(act(w, "p").wait == WRecreation and act(w, "p").suspend == 1 and invariant)
      .then(sysStep("c1", true)).then(sysStep("c1", true)).then(sysStep("p", true))
      .expect(act(w, "p").wait == WNone and act(w, "p").suspend == 0 and phaseOf("c1") == Free and invariant)

  // A parent that watches its child handles the child's notice before the
  // child's Terminated (T4).
  run aWatchingParentLearnsOfItsChildFirstTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(stop("c1")).then(sysStep("c1", true)).then(tellRest("c1")).then(sysStep("p", true))
      .then(receive("p", false, DRestart))
      .expect(parentLearnsBeforeWatchers and invariant)

  // ---- runs: the fixed design, stage 4b ------------------------------------

  // #251: a start failure whose decider fails is the parent's failure; the
  // parent's restart recreates the child it keeps, and the retried start runs
  // in the child's own run, after the parent's restart has finished (depth 1).
  run anEscalatedStartFailureIsRetriedAfterTheParentRestartTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", false))
      .then(runSys("p", true, DEscalate, DRestart, false))
      .expect(act(w, "p").failed == "c1" and act(w, "p").sys == [Recreate] and act(w, "c1").sys == []
        and act(w, "c1").suspend == 1 and invariant)
      .then(sysStep("p", true))
      .expect(act(w, "p").restarting == 0 and act(w, "p").suspend == 0 and act(w, "c1").sys == [Recreate]
        and recoveryNotNested and invariant)
      .then(sysStep("c1", true))
      .expect(phaseOf("c1") == Alive and act(w, "c1").suspend == 0 and invariant)

  // Stage 4a's two queued retries, the Pekko way: the decision on c1's
  // failed start sends a Recreate; p fails, suspends c1 and is restarted, and
  // its finishRecreate sends c1 another Recreate. The first retry fails again
  // and tells p; the second creates the instance; the decision on the second
  // failure restarts it. Every suspend is answered.
  run twoRecreatesBehindAFailedStartAreBalancedTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", false))
      .then(sysStep("p", true)).then(send("p")).then(receive("p", true, DRestart)).then(sysStep("p", true))
      .expect(act(w, "c1").sys == [Recreate, Suspend, Recreate] and invariant)
      .then(sysStep("c1", false)).then(sysStep("c1", true)).then(sysStep("c1", true))
      .expect(phaseOf("c1") == Alive and act(w, "c1").suspend == 1 and act(w, "p").sys.length() == 1 and invariant)
      .then(sysStep("p", true)).then(sysStep("c1", true))
      .expect(phaseOf("c1") == Alive and act(w, "c1").suspend == 0 and act(w, "c1").restarts == 1 and settled
        and invariant)

  // #254, 1: a Recreate that reaches c1 while it waits for g (its own
  // restart) is kept aside and runs after that restart: c1 restarts twice.
  run aRecreateWhileWaitingForChildrenRunsAfterTheWaitTest =
    initWith(Fixed, true).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(spawn("c2")).then(sysStep("c2", true)).then(spawn("g")).then(sysStep("g", true))
      .then(send("c1")).then(receive("c1", true, DRestart)).then(sysStep("p", true))
      .then(runSys("c1", true, DRestart, DRestart, true))
      .expect(act(w, "c1").wait == WRecreation and invariant)
      .then(sysStep("c2", true)).then(sysStep("c2", true)).then(send("c2")).then(receive("c2", true, DRestart))
      .then(sysStep("p", true))
      .expect(act(w, "c1").sys == [Suspend, Recreate] and invariant)
      .then(sysStep("c1", true)).then(sysStep("c1", true))
      .expect(act(w, "c1").stash == [Suspend, Recreate] and act(w, "c1").sys == [] and invariant)
      .then(sysStep("g", true)).then(sysStep("g", true)).then(sysStep("c1", true))
      .expect(act(w, "c1").restarts == 1 and act(w, "c1").sys == [Suspend, Recreate] and act(w, "c1").stash == []
        and invariant)
      .then(sysStep("c1", true)).then(sysStep("c1", true))
      .expect(act(w, "c1").restarts == 2 and act(w, "c1").suspend == 0 and recoveryNotNested and invariant)

  // #254, 2: under all-for-one, a sibling that has not handled its Create is
  // suspended and recreated like the others: Create runs preStart, then the
  // restart runs preRestart and postRestart and counts.
  run anAllForOneRestartRestartsASiblingThatHasNotStartedTest =
    initWith(Fixed, true).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(spawn("c2")).then(send("c1")).then(receive("c1", true, DRestart)).then(sysStep("p", true))
      .expect(act(w, "c2").sys == [Create, Suspend, Recreate] and act(w, "c1").sys == [Recreate] and invariant)
      .then(sysStep("c2", true)).then(sysStep("c2", true)).then(sysStep("c2", true)).then(sysStep("c1", true))
      .expect(act(w, "c2").restarts == 1 and act(w, "c2").suspend == 0 and act(w, "c1").restarts == 1 and settled
        and invariant)

  // A parent whose preRestart keeps its children recreates them once it has
  // restarted (finishRecreate's survivors); its failure suspended them first.
  run aParentsRestartRecreatesTheChildrenItKeepsTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(send("p")).then(receive("p", true, DRestart))
      .expect(act(w, "c1").sys == [Suspend] and act(w, "p").sys == [Recreate] and invariant)
      .then(sysStep("c1", true)).then(sysStep("p", true)).then(sysStep("c1", true))
      .expect(act(w, "c1").restarts == 1 and act(w, "c1").suspend == 0 and act(w, "p").restarts == 1 and settled
        and invariant)

  // A child's Failed waits while its parent has no instance (StashWhenFailed),
  // and a Failed from an incarnation the parent no longer holds is dropped
  // (handleFailure's uid check).
  run aFailedFromAnOldChildIsDroppedTest =
    initWith(Fixed, false).then(spawn("p")).then(spawn("c1")).then(sysStep("c1", true)).then(send("c1"))
      .then(receive("c1", true, DRestart)).then(sysStep("p", false))
      .expect(phaseOf("p") == StartFailed and act(w, "p").sys.length() == 2 and invariant)
      .then(sysStep("p", true))
      .expect(act(w, "p").stash.length() == 1 and invariant)
      .then(sysStep("p", true)).then(sysStep("c1", true)).then(sysStep("c1", true)).then(tellRest("c1"))
      .then(sysStep("p", true))
      .expect(phaseOf("p") == Alive and act(w, "p").stash == [] and act(w, "p").sys.length() == 1 and invariant)
      .then(spawn("c1")).then(sysStep("p", true))
      .expect(act(w, "c1").sys == [Create] and act(w, "c1").inc == 2 and invariant)

  // Children.makeChild: c2 is created while p's failure waits for the
  // guardian's Recreate, so it gets p's one suspend as a Suspend behind its
  // Create, and p's finishRecreate answers it with the Recreate it sends to
  // every survivor.
  run aChildCreatedUnderASuspendedParentGetsItsSuspendsTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(send("p")).then(receive("p", true, DRestart)).then(spawn("c2"))
      .expect(act(w, "c2").sys == [Create, Suspend] and act(w, "p").sys == [Recreate] and invariant)
      .then(sysStep("c2", true)).then(sysStep("c2", true)).then(sysStep("c1", true))
      .expect(act(w, "c2").suspend == 1 and invariant)
      .then(sysStep("p", true)).then(sysStep("c1", true)).then(sysStep("c2", true))
      .expect(act(w, "c1").restarts == 1 and act(w, "c2").restarts == 1 and act(w, "c2").suspend == 0 and settled
        and invariant)

  // The same while p waits in its restart for c2 to stop: c1, stopped by
  // preRestart, is created again (as a handleChildTerminated hook would) and
  // is suspended until the Recreate of p's finishRecreate.
  run aChildCreatedWhileTheParentWaitsIsRecreatedSuspendedTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(spawn("c2")).then(sysStep("c2", true)).then(send("p")).then(receive("p", true, DRestart))
      .then(runSys("p", true, DRestart, DRestart, true)).then(sysStep("c1", true)).then(sysStep("c1", true))
      .then(tellRest("c1")).then(sysStep("p", true)).then(spawn("c1"))
      .expect(act(w, "c1").inc == 2 and act(w, "c1").sys == [Create, Suspend] and act(w, "p").wait == WRecreation
        and invariant)
      .then(sysStep("c1", true)).then(sysStep("c1", true)).then(sysStep("c2", true)).then(sysStep("c2", true))
      .then(tellRest("c2")).then(sysStep("p", true))
      .expect(act(w, "c1").sys == [Recreate] and act(w, "c1").suspend == 1 and invariant)
      .then(sysStep("c1", true))
      .expect(act(w, "c1").restarts == 1 and act(w, "c1").suspend == 0 and act(w, "p").restarts == 1 and settled
        and invariant)

  // The guardian's decider fails on p's escalation of c1's failure and
  // resumes p; p's faultResume passes the cause on to c1, the perpetrator,
  // which clears c1's failure, and g, which c1's failure suspended, is
  // resumed and receives.
  run aResumeAnswersThePerpetratorsFailureTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(spawn("g")).then(sysStep("g", true)).then(send("c1")).then(receive("c1", true, DRestart))
      .then(runSys("p", true, DEscalate, DEscalate, false))
      .expect(act(w, "p").failed == "c1" and act(w, "p").sys == [Resume(true)] and invariant)
      .then(sysStep("p", true))
      .expect(act(w, "c1").sys == [Resume(true)] and invariant)
      .then(sysStep("g", true)).then(sysStep("c1", true)).then(sysStep("g", true)).then(send("g"))
      .then(receive("g", false, DRestart))
      .expect(act(w, "c1").failed == "" and act(w, "c1").suspend == 0 and act(w, "g").suspend == 0 and settled
        and invariant)

  // c1's postRestart fails while c1 holds g's Failed aside (state 1). The
  // stop runs in the same run: c1 waits for g in Termination, keeps the
  // Failed aside, and closes without deciding it.
  run aFailedPostRestartStopsBeforeAKeptFailedTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(spawn("g")).then(send("c1")).then(receive("c1", true, DRestart)).then(sysStep("g", false))
      .then(sysStep("c1", true))
      .expect(act(w, "c1").stash.length() == 1 and invariant)
      .then(sysStep("p", true)).then(sysStep("c1", false))
      .expect(act(w, "c1").fatal and act(w, "c1").wait == WTermination and act(w, "c1").stash.length() == 1
        and act(w, "c1").sys == [] and invariant)
      .then(sysStep("g", true)).then(sysStep("g", true)).then(tellRest("g")).then(sysStep("c1", true))
      .then(tellRest("c1")).then(sysStep("p", true))
      .expect(phaseOf("c1") == Free and phaseOf("g") == Free and act(w, "c1").postStops == 0 and invariant)

  // #254, 1 with a failed postRestart: c1 holds a Suspend and a Recreate
  // aside while it waits for g; the postRestart of that restart fails, c1
  // has no child left and closes in the same run, and the kept Recreate never
  // runs.
  run aFailedPostRestartStopsBeforeAKeptRecreateTest =
    initWith(Fixed, true).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(spawn("c2")).then(sysStep("c2", true)).then(spawn("g")).then(sysStep("g", true))
      .then(send("c1")).then(receive("c1", true, DRestart)).then(sysStep("p", true))
      .then(runSys("c1", true, DRestart, DRestart, true)).then(sysStep("c2", true)).then(sysStep("c2", true))
      .then(send("c2")).then(receive("c2", true, DRestart)).then(sysStep("p", true))
      .then(sysStep("c1", true)).then(sysStep("c1", true))
      .expect(act(w, "c1").stash == [Suspend, Recreate] and invariant)
      .then(sysStep("g", true)).then(sysStep("g", true)).then(sysStep("c1", false))
      .expect(phaseOf("c1") == Closed and act(w, "c1").restarts == 0 and act(w, "c1").postStops == 0 and invariant)

  // ---- runs: the mistakes, stage 4a ---------------------------------------

  run closeWithChildrenLosesTheChildsNoticeTest =
    initWith(CloseWithChildren, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(stop("p")).then(sysStep("p", true))
      .expect(not(noCloseBeforeChildren))
      .then(sysStep("c1", true))
      .expect(not(noLostChildTerminated))

  run notifyBeforePostStopTellsTheWatcherTooEarlyTest =
    initWith(NotifyBeforePostStop, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(stop("p")).then(sysStep("p", true))
      .expect(not(noticesAfterTheEnd))

  run watchersFirstLetsTheParentSeeTerminatedFirstTest =
    initWith(WatchersFirst, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(stop("c1")).then(sysStep("c1", true)).then(receive("p", false, DRestart))
      .expect(not(parentLearnsBeforeWatchers))

  run earlyNameReleaseFreesTheNameOfALiveActorTest =
    initWith(EarlyNameRelease, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(spawn("g")).then(sysStep("g", true)).then(stop("c1")).then(sysStep("c1", true))
      .expect(not(namesHeldWhileLive))

  run syncStopClosesBeforeTheCreateTest =
    initWith(SyncStop, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(stop("c1"))
      .expect(not(createBeforeTerminate))

  run keepWatchingSendsTerminatedToAStoppingActorTest =
    initWith(KeepWatching, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(stop("p")).then(sysStep("p", true)).then(sysStep("c1", true))
      .then(tellRest("c1"))
      .expect(not(noTerminatedToAStoppingWatcher))

  run noCreationWaitKeepsTheAttemptsChildrenTest =
    initWith(NoCreationWait, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(spawn("g"))
      .then(sysStep("c1", false)).then(sysStep("p", true)).then(sysStep("c1", true))
      .expect(not(retryWithoutTheAttemptsChildren))

  run noRecreationWaitRestartsBeforeTheChildrenGoTest =
    initWith(NoRecreationWait, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(send("p")).then(receive("p", true, DRestart))
      .then(runSys("p", true, DRestart, DRestart, true))
      .expect(not(restartAfterStoppedChildren))

  run noTerminateToChildrenWaitsForeverTest =
    initWith(NoTerminateToChildren, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(stop("p")).then(sysStep("p", true))
      .expect(not(waitsEnd))

  run notifyTwiceTellsTheWatcherTwiceTest =
    initWith(NotifyTwice, false).then(spawn("p")).then(sysStep("p", true)).then(stop("p")).then(sysStep("p", true))
      .then(tellRest("p"))
      .expect(not(noticesOnce))

  run keepSuspendedLeavesARestartedActorSuspendedTest =
    initWith(KeepSuspended, false).then(spawn("p")).then(sysStep("p", true)).then(send("p"))
      .then(receive("p", true, DRestart)).then(sysStep("p", true))
      .expect(not(settledMeansResumed))

  // ---- runs: the mistakes, stage 4b ---------------------------------------

  // p fails and does not suspend c1, so c1 receives before p's decision.
  run noSuspendChildrenLetsAChildReceiveUnderAFailedParentTest =
    initWith(NoSuspendChildren, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(send("p")).then(receive("p", true, DRestart)).then(send("c1"))
      .then(receive("c1", false, DRestart))
      .expect(not(noReceiveUnderAFailedParent))

  // p's start failed; c1's Failed is decided at once, without p's instance.
  run noStashWhenFailedDecidesWithoutAnInstanceTest =
    initWith(NoStashWhenFailed, false).then(spawn("p")).then(spawn("c1")).then(sysStep("c1", true)).then(send("c1"))
      .then(receive("c1", true, DRestart)).then(sysStep("p", false)).then(sysStep("p", true))
      .expect(not(decideWithInstance))

  // c1 waits for g in its restart and handles the next Recreate at once: a
  // second restart begins inside the first.
  run noStashWhileWaitingNestsTheRestartTest =
    initWith(NoStashWhileWaiting, true).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(spawn("c2")).then(sysStep("c2", true)).then(spawn("g")).then(sysStep("g", true))
      .then(send("c1")).then(receive("c1", true, DRestart)).then(sysStep("p", true))
      .then(runSys("c1", true, DRestart, DRestart, true)).then(sysStep("c2", true)).then(sysStep("c2", true))
      .then(send("c2")).then(receive("c2", true, DRestart)).then(sysStep("p", true))
      .then(sysStep("c1", true)).then(sysStep("c1", true))
      .expect(not(recoveryNotNested))

  // Stage 4a's skip: the Recreate is dropped while c1 waits, the Suspend
  // before it is kept and replayed, and nothing resumes c1.
  run dropRecreateWhileWaitingLeavesTheActorSuspendedTest =
    initWith(DropRecreateWhileWaiting, true).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(spawn("c2")).then(sysStep("c2", true)).then(spawn("g")).then(sysStep("g", true))
      .then(send("c1")).then(receive("c1", true, DRestart)).then(sysStep("p", true))
      .then(runSys("c1", true, DRestart, DRestart, true)).then(sysStep("c2", true)).then(sysStep("c2", true))
      .then(send("c2")).then(receive("c2", true, DRestart)).then(sysStep("p", true))
      .then(sysStep("c1", true)).then(sysStep("c1", true)).then(sysStep("g", true)).then(sysStep("g", true))
      .then(tellRest("g")).then(sysStep("c1", true)).then(sysStep("c1", true)).then(sysStep("c2", true))
      .expect(not(settledMeansResumed))

  // Stage 4a's guard: the second Recreate finds the failed start undecided
  // and does nothing, so the Suspend before it is never answered.
  run ignoreUndecidedRecreateLeavesTheChildSuspendedTest =
    initWith(IgnoreUndecidedRecreate, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", false)).then(sysStep("p", true)).then(send("p")).then(receive("p", true, DRestart))
      .then(sysStep("p", true)).then(sysStep("c1", false)).then(sysStep("c1", true)).then(sysStep("c1", true))
      .then(sysStep("p", true)).then(sysStep("c1", true))
      .expect(not(settledMeansResumed))

  // Stage 4a's form: p's restart keeps c1 but does not recreate it, so the
  // Suspend of p's failure is never answered.
  run noSurvivorRecreateLeavesTheChildSuspendedTest =
    initWith(NoSurvivorRecreate, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(send("p")).then(receive("p", true, DRestart)).then(sysStep("c1", true))
      .then(sysStep("p", true))
      .expect(not(settledMeansResumed))

  // The Failed of c1's first incarnation is decided for its second one,
  // which is not suspended.
  run noChildCheckRecreatesAnUnsuspendedChildTest =
    initWith(NoChildCheck, false).then(spawn("p")).then(spawn("c1")).then(sysStep("c1", true)).then(send("c1"))
      .then(receive("c1", true, DRestart)).then(sysStep("p", false)).then(sysStep("p", true))
      .then(sysStep("p", true)).then(sysStep("c1", true)).then(sysStep("c1", true)).then(tellRest("c1"))
      .then(sysStep("p", true)).then(spawn("c1")).then(sysStep("p", true)).then(sysStep("c1", true))
      .then(sysStep("c1", true))
      .expect(not(recreateWhileSuspended))

  // c2 is created under p while p's failure is unanswered and gets no
  // Suspend, so it receives before the guardian's decision on p.
  run noSuspendInheritanceLetsANewChildReceiveUnderAFailedParentTest =
    initWith(NoSuspendInheritance, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(send("p")).then(receive("p", true, DRestart)).then(spawn("c2"))
      .then(sysStep("c2", true)).then(send("c2")).then(receive("c2", false, DRestart))
      .expect(not(noReceiveUnderAFailedParent))

  // p's faultResume resumes c1 without the cause, so c1 keeps its failure,
  // and g receives under it.
  run resumeWithoutCauseLeavesThePerpetratorFailedTest =
    initWith(ResumeWithoutCause, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", true)).then(spawn("g")).then(sysStep("g", true)).then(send("c1"))
      .then(receive("c1", true, DRestart)).then(runSys("p", true, DEscalate, DEscalate, false)).then(sysStep("p", true))
      .then(sysStep("g", true)).then(sysStep("c1", true)).then(sysStep("g", true)).then(send("g"))
      .then(receive("g", false, DRestart))
      .expect(not(noReceiveUnderAFailedParent))

  // All-for-one without the Suspend first: c2 runs faultRecreate unsuspended.
  run noSuspendFirstRecreatesAnUnsuspendedSiblingTest =
    initWith(NoSuspendFirst, true).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(spawn("c2")).then(sysStep("c2", true)).then(send("c1")).then(receive("c1", true, DRestart))
      .then(sysStep("p", true)).then(sysStep("c2", true))
      .expect(not(recreateWhileSuspended))

  // p's decider fails on c1's failed start and p's failure suspends c1 a
  // second time; the one Recreate of p's finishRecreate leaves c1 suspended.
  run suspendThePerpetratorLeavesItSuspendedTest =
    initWith(SuspendThePerpetrator, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", false)).then(runSys("p", true, DEscalate, DRestart, false)).then(sysStep("p", true))
      .expect(act(w, "c1").sys == [Suspend, Recreate])
      .then(sysStep("c1", true)).then(sysStep("c1", true))
      .expect(not(settledMeansResumed))

  // A failed postRestart that only queues Terminate: c1's state goes down,
  // g's kept Failed comes back before the Terminate, and c1 decides it.
  run stopAtTheTailDecidesAKeptFailedTest =
    initWith(StopAtTheTail, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(spawn("g")).then(send("c1")).then(receive("c1", true, DRestart)).then(sysStep("g", false))
      .then(sysStep("c1", true)).then(sysStep("p", true)).then(sysStep("c1", false))
      .expect(act(w, "c1").sys.length() == 2 and act(w, "c1").sys.head() != Terminate)
      .then(sysStep("c1", true))
      .expect(not(aFailedRestartOnlyStops))

  // An actor waiting in Creation for the child of its first failed attempt
  // stays in that wait until the child has gone, and then starts again; a
  // Recreate that reaches it inside that wait cannot be made by this model
  // (only a parent outside its steps could send one), so the run pins the
  // wait and its child, and the mistake about it is below.
  run aRecreateWhileWaitingInCreationIsKeptTest =
    initWith(Fixed, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(spawn("g"))
      .then(sysStep("c1", false)).then(sysStep("g", true)).then(sysStep("g", true)).then(sysStep("p", true))
      .then(sysStep("c1", true))
      .expect(act(w, "c1").wait == WCreation and act(w, "c1").phase == StartFailed and act(w, "c1").children == Set("g"))
      .then(sysStep("g", true)).then(sysStep("c1", true))
      .expect(act(w, "c1").phase == Alive and act(w, "c1").children == Set() and act(w, "c1").wait == WNone)

  // The mistake: a Recreate runs inside a wait instead of being kept aside
  // (shouldStash lets it through for this mode, Creation included). The model
  // puts no Recreate in c1's Creation wait, so here the mode behaves as
  // NoStashWhileWaiting; the run pins the wait.
  run handleRecreateUnderCreationRunsInsideTheWaitTest =
    initWith(HandleRecreateUnderCreation, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(spawn("g")).then(sysStep("c1", false)).then(sysStep("g", true)).then(sysStep("g", true))
      .then(sysStep("p", true)).then(sysStep("c1", true))
      .expect(act(w, "c1").wait == WCreation and act(w, "c1").children == Set("g") and act(w, "c1").stash == [])
      .then(sysStep("g", false))
      .expect(act(w, "c1").wait == WCreation and act(w, "c1").phase == StartFailed and act(w, "c1").children == Set("g"))

  // The same with #254, 1: the kept Recreate runs before the Terminate and
  // restarts the actor whose postRestart failed.
  run stopAtTheTailRunsAKeptRecreateTest =
    initWith(StopAtTheTail, true).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1")).then(sysStep("c1", true))
      .then(spawn("c2")).then(sysStep("c2", true)).then(spawn("g")).then(sysStep("g", true))
      .then(send("c1")).then(receive("c1", true, DRestart)).then(sysStep("p", true))
      .then(runSys("c1", true, DRestart, DRestart, true)).then(sysStep("c2", true)).then(sysStep("c2", true))
      .then(send("c2")).then(receive("c2", true, DRestart)).then(sysStep("p", true))
      .then(sysStep("c1", true)).then(sysStep("c1", true)).then(sysStep("g", true)).then(sysStep("g", true))
      .then(sysStep("c1", false))
      .expect(act(w, "c1").sys == [Suspend, Recreate, Terminate])
      .then(sysStep("c1", true)).then(sysStep("c1", true))
      .expect(not(aFailedRestartOnlyStops))

  // #319: p's decider fails on c1's failed start, and p, a child of the
  // guardian, tells nobody. p waits for an answer that never comes, so p and
  // the child it suspended stay suspended once nothing is in progress. The
  // fixed design reports p's failure to the guardian, whose decision restarts
  // p (anEscalatedStartFailureIsRetriedAfterTheParentRestartTest).
  run untoldEscalationLeavesTheParentSuspendedTest =
    initWith(UntoldEscalation, false).then(spawn("p")).then(sysStep("p", true)).then(spawn("c1"))
      .then(sysStep("c1", false)).then(runSys("p", true, DEscalate, DRestart, false))
      .expect(act(w, "p").failed == "c1" and act(w, "p").suspend == 1 and act(w, "p").sys == [] and settled)
      .expect(not(settledMeansResumed))
}
