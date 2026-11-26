// function_ref (crates/fraktor-actor/src/kernel/actor, the FunctionRef
// of #192: ActorContext::add_function_ref / remove_function_ref and the
// executor's function ref registry).
//
// Actors are 1 (SupSuper) and 2 (Super, a child of 1); function refs
// carry consecutive ids from 0 and are never reused. `registered` is
// the live registry, `parentOf` maps a ref to the actor that created
// it, `deliveryWatchers` is the ordinary DeathWatch table (a pair
// (watcher, target); notifying removes the pair), and `watching` is
// the function refs' own table, which survives a notice until the ref
// unwatches or stops (design F3). `terminatedList` records every
// Terminated delivery in order, so "exactly once" can be stated even
// though a set would hide a duplicate. `mustStayWatching` is a ghost
// set: when a target ends, its function ref watchers are recorded
// there, and unwatch or the watcher's own stop removes the pair
// again; the invariant says those pairs are still in `watching`.
// Watching a ref that already ended answers with a notice on every
// call, as the code does, so "exactly once" is stated over the
// notices one end causes (`endNotices`), not over those answers.
// `runLog` and `endLog` record handler runs and ref ends with a
// logical clock, so `noForwardAfterRemoval` can compare the two.
// Refs end one at a time in registration order (increasing id), as
// the executor ends them: a watcher is told about a ref's end only
// if the watcher itself has not ended earlier in the same sequence
// (`noticesForSeq`).
module function_ref {
  var registered: Set[int]
  var parentOf: Set[(int, int)]
  var aliveActors: Set[int]
  var actorParent: Set[(int, int)]
  var deliveryWatchers: Set[(int, int)]
  var watching: Set[(int, int)]
  var terminatedList: List[(int, int)]
  var endNotices: List[(int, int)]
  var mustStayWatching: Set[(int, int)]
  var nextRef: int
  var forwarded: int
  var deadMessages: int
  var runLog: List[(int, int)]
  var endLog: List[(int, int)]
  var clock: int

  pure val Probe = 999

  pure def parentOfRef(parents: Set[(int, int)], r: int): int =
    parents.filter(p => p._1 == r).fold(-1, (acc, p) => p._2)

  action init = all {
    registered' = Set(),
    parentOf' = Set(),
    aliveActors' = Set(1, 2),
    actorParent' = Set((2, 1)),
    deliveryWatchers' = Set(),
    watching' = Set(),
    terminatedList' = [],
    endNotices' = [],
    mustStayWatching' = Set(),
    nextRef' = 0,
    forwarded' = 0,
    deadMessages' = 0,
    runLog' = [],
    endLog' = [],
    clock' = 0,
  }

  // The watchers of `r` in both tables.
  pure def watchersOf(dw: Set[(int, int)], w: Set[(int, int)], r: int): Set[int] =
    dw.filter(p => p._2 == r).map(p => p._1).union(w.filter(p => p._2 == r).map(p => p._1))

  // The deliveries one end of `r` makes, in a fixed order (watchers in
  // increasing id order is not needed; a list built from the set is
  // enough because the invariant only counts).
  pure def noticesFor(dw: Set[(int, int)], w: Set[(int, int)], r: int): List[(int, int)] =
    watchersOf(dw, w, r).fold([], (acc, watcher) => acc.append((watcher, r)))

  // The deliveries one end of `r` makes while `refs` end in
  // registration order: a watcher counts only if it has not ended
  // earlier in the same sequence (a smaller id in `refs`) and is not
  // `r` itself.
  pure def noticesForSeq(dw: Set[(int, int)], w: Set[(int, int)], refs: Set[int], r: int): List[(int, int)] =
    watchersOf(dw, w, r)
      .filter(x => x != r and not(refs.contains(x) and x < r))
      .fold([], (acc, watcher) => acc.append((watcher, r)))

  // Ends `refs` at once: notifies their watchers, drops the delivery
  // registrations of and by them, keeps the `watching` pairs whose
  // target ended (F3), and clears the watching pairs they held.
  action endRefs(refs: Set[int]): bool = all {
    registered' = registered.filter(x => not(refs.contains(x))),
    terminatedList' = refs.fold(terminatedList, (acc, r) => acc.concat(noticesForSeq(deliveryWatchers, watching, refs, r))),
    endNotices' = refs.fold(endNotices, (acc, r) => acc.concat(noticesForSeq(deliveryWatchers, watching, refs, r))),
    deliveryWatchers' = deliveryWatchers.filter(p => not(refs.contains(p._2)) and not(refs.contains(p._1))),
    watching' = watching.filter(p => not(refs.contains(p._1))),
    mustStayWatching' = mustStayWatching.filter(p => not(refs.contains(p._1)))
      .union(refs.fold(Set(), (s, r) => s.union(watching.filter(p => p._2 == r and not(refs.contains(p._1)))))),
    parentOf' = parentOf,
    aliveActors' = aliveActors,
    actorParent' = actorParent,
    nextRef' = nextRef,
    endLog' = refs.fold(endLog, (acc, r) => acc.append((clock, r))),
    clock' = clock + 1,
    forwarded' = forwarded,
    deadMessages' = deadMessages,
    runLog' = runLog,
  }

  action addRef(parent: int): bool = all {
    aliveActors.contains(parent),
    registered' = registered.union(Set(nextRef)),
    parentOf' = parentOf.union(Set((nextRef, parent))),
    nextRef' = nextRef + 1,
    aliveActors' = aliveActors,
    actorParent' = actorParent,
    deliveryWatchers' = deliveryWatchers,
    watching' = watching,
    terminatedList' = terminatedList,
    endNotices' = endNotices,
    mustStayWatching' = mustStayWatching,
    forwarded' = forwarded,
    deadMessages' = deadMessages,
    runLog' = runLog,
    endLog' = endLog,
    clock' = clock,
  }

  action addRefStep = {
    nondet parent = Set(1, 2).oneOf()
    addRef(parent)
  }

  action removeRef(r: int): bool = all {
    registered.contains(r),
    endRefs(Set(r)),
  }

  action removeRefStep = {
    nondet r = registered.oneOf()
    removeRef(r)
  }

  // Stopping an actor stops its child actors (one chain link at a
  // time is enough for the two-actor tree) and every ref they created.
  action stopActor(a: int): bool = all {
    aliveActors.contains(a),
    val children = aliveActors.filter(x => parentOfRef(actorParent, x) == a)
    val stoppedActors = Set(a).union(children)
      .union(aliveActors.filter(x => children.contains(parentOfRef(actorParent, x))))
    val refs = registered.filter(r => stoppedActors.contains(parentOfRef(parentOf, r)))
    all {
      registered' = registered.filter(x => not(refs.contains(x))),
      terminatedList' = refs.fold(terminatedList, (acc, r) => acc.concat(noticesForSeq(deliveryWatchers, watching, refs, r))),
      endNotices' = refs.fold(endNotices, (acc, r) => acc.concat(noticesForSeq(deliveryWatchers, watching, refs, r))),
      deliveryWatchers' = deliveryWatchers.filter(p => not(refs.contains(p._2)) and not(refs.contains(p._1))),
      watching' = watching.filter(p => not(refs.contains(p._1))),
      mustStayWatching' = mustStayWatching.filter(p => not(refs.contains(p._1)))
        .union(refs.fold(Set(), (s, r) => s.union(watching.filter(p => p._2 == r and not(refs.contains(p._1)))))),
      parentOf' = parentOf,
      aliveActors' = aliveActors.filter(x => not(stoppedActors.contains(x))),
      actorParent' = actorParent,
      nextRef' = nextRef,
      endLog' = refs.fold(endLog, (acc, r) => acc.append((clock, r))),
      clock' = clock + 1,
      forwarded' = forwarded,
      deadMessages' = deadMessages,
      runLog' = runLog,
    }
  }

  action stopActorStep = {
    nondet a = aliveActors.oneOf()
    stopActor(a)
  }

  // The orphan defect: the actor stops but its refs stay registered.
  action stopActorBugOrphan(a: int): bool = all {
    aliveActors.contains(a),
    aliveActors' = aliveActors.filter(x => x != a),
    registered' = registered,
    parentOf' = parentOf,
    actorParent' = actorParent,
    deliveryWatchers' = deliveryWatchers,
    watching' = watching,
    terminatedList' = terminatedList,
    endNotices' = endNotices,
    mustStayWatching' = mustStayWatching,
    nextRef' = nextRef,
    forwarded' = forwarded,
    deadMessages' = deadMessages,
    runLog' = runLog,
    endLog' = endLog,
    clock' = clock,
  }

  action watchByActor(r: int): bool = all {
    registered.contains(r),
    deliveryWatchers' = deliveryWatchers.union(Set((Probe, r))),
    registered' = registered,
    parentOf' = parentOf,
    aliveActors' = aliveActors,
    actorParent' = actorParent,
    watching' = watching,
    terminatedList' = terminatedList,
    endNotices' = endNotices,
    mustStayWatching' = mustStayWatching,
    nextRef' = nextRef,
    forwarded' = forwarded,
    deadMessages' = deadMessages,
    runLog' = runLog,
    endLog' = endLog,
    clock' = clock,
  }

  action watchByActorStep = {
    nondet r = registered.oneOf()
    watchByActor(r)
  }

  // A function ref watching another ref. Watching a ref that already
  // ended answers with a notice at once, as the code does.
  action watchByRef(w: int, r: int): bool = all {
    registered.contains(w),
    watching' = if (registered.contains(r)) watching.union(Set((w, r))) else watching,
    terminatedList' = if (registered.contains(r)) terminatedList else terminatedList.append((w, r)),
    endNotices' = endNotices,
    registered' = registered,
    parentOf' = parentOf,
    aliveActors' = aliveActors,
    actorParent' = actorParent,
    deliveryWatchers' = deliveryWatchers,
    mustStayWatching' = mustStayWatching,
    nextRef' = nextRef,
    forwarded' = forwarded,
    deadMessages' = deadMessages,
    runLog' = runLog,
    endLog' = endLog,
    clock' = clock,
  }

  action watchByRefStep = {
    nondet w = registered.oneOf()
    nondet r = 0.to(nextRef - 1).oneOf()
    watchByRef(w, r)
  }

  action unwatchByRef(w: int, r: int): bool = all {
    watching' = watching.filter(p => p != (w, r)),
    mustStayWatching' = mustStayWatching.filter(p => p != (w, r)),
    registered' = registered,
    parentOf' = parentOf,
    aliveActors' = aliveActors,
    actorParent' = actorParent,
    deliveryWatchers' = deliveryWatchers,
    terminatedList' = terminatedList,
    endNotices' = endNotices,
    nextRef' = nextRef,
    forwarded' = forwarded,
    deadMessages' = deadMessages,
    runLog' = runLog,
    endLog' = endLog,
    clock' = clock,
  }

  action unwatchByRefStep = {
    nondet p = watching.oneOf()
    unwatchByRef(p._1, p._2)
  }

  // A message sent to a ref runs its function only while the ref is
  // registered; a message to an ended ref goes to dead letters. Both
  // outcomes are recorded with the current clock, so the invariant
  // can compare a run against the ref's end.
  action sendTo(r: int): bool = all {
    forwarded' = if (registered.contains(r)) forwarded + 1 else forwarded,
    deadMessages' = if (registered.contains(r)) deadMessages else deadMessages + 1,
    runLog' = if (registered.contains(r)) runLog.append((clock, r)) else runLog,
    endLog' = endLog,
    clock' = clock + 1,
    registered' = registered,
    parentOf' = parentOf,
    aliveActors' = aliveActors,
    actorParent' = actorParent,
    deliveryWatchers' = deliveryWatchers,
    watching' = watching,
    terminatedList' = terminatedList,
    endNotices' = endNotices,
    mustStayWatching' = mustStayWatching,
    nextRef' = nextRef,
  }

  action sendToStep = {
    nondet r = 0.to(nextRef - 1).oneOf()
    sendTo(r)
  }

  // The defect that runs the function of a ref that already ended.
  action sendToBugRunsEnded(r: int): bool = all {
    forwarded' = forwarded + 1,
    deadMessages' = deadMessages,
    runLog' = runLog.append((clock, r)),
    endLog' = endLog,
    clock' = clock + 1,
    registered' = registered,
    parentOf' = parentOf,
    aliveActors' = aliveActors,
    actorParent' = actorParent,
    deliveryWatchers' = deliveryWatchers,
    watching' = watching,
    terminatedList' = terminatedList,
    endNotices' = endNotices,
    mustStayWatching' = mustStayWatching,
    nextRef' = nextRef,
  }

  // The double-cleanup defect: ending a ref notifies, and the parent
  // stop notifies the same watchers again.
  action removeRefBugDoubleNotify(r: int): bool = all {
    registered.contains(r),
    val notices = noticesFor(deliveryWatchers, watching, r)
    all {
      registered' = registered.filter(x => x != r),
      terminatedList' = terminatedList.concat(notices).concat(notices),
      endNotices' = endNotices.concat(notices).concat(notices),
      deliveryWatchers' = deliveryWatchers.filter(p => p._2 != r and p._1 != r),
      watching' = watching.filter(p => p._1 != r),
      mustStayWatching' = mustStayWatching.filter(p => p._1 != r)
        .union(watching.filter(p => p._2 == r and p._1 != r).map(p => p)),
      parentOf' = parentOf,
      aliveActors' = aliveActors,
      actorParent' = actorParent,
      nextRef' = nextRef,
      forwarded' = forwarded,
    deadMessages' = deadMessages,
    runLog' = runLog,
    endLog' = endLog,
    clock' = clock,
    }
  }

  // The auto-unwatch defect: a notice clears the watching pair.
  action removeRefBugAutoUnwatch(r: int): bool = all {
    registered.contains(r),
    registered' = registered.filter(x => x != r),
    terminatedList' = terminatedList.concat(noticesFor(deliveryWatchers, watching, r)),
    endNotices' = endNotices.concat(noticesFor(deliveryWatchers, watching, r)),
    deliveryWatchers' = deliveryWatchers.filter(p => p._2 != r and p._1 != r),
    watching' = watching.filter(p => p._1 != r and p._2 != r),
    mustStayWatching' = mustStayWatching.filter(p => p._1 != r)
      .union(watching.filter(p => p._2 == r and p._1 != r).map(p => p)),
    parentOf' = parentOf,
    aliveActors' = aliveActors,
    actorParent' = actorParent,
    nextRef' = nextRef,
    forwarded' = forwarded,
    deadMessages' = deadMessages,
    runLog' = runLog,
    endLog' = endLog,
    clock' = clock,
  }

  action step = any {
    init, addRefStep, removeRefStep, stopActorStep,
    watchByActorStep, watchByRefStep, unwatchByRefStep, sendToStep,
  }

  def endNoticeSet: Set[(int, int)] = endNotices.foldl(Set(), (s, p) => s.union(Set(p)))
  def terminatedSet: Set[(int, int)] = terminatedList.foldl(Set(), (s, p) => s.union(Set(p)))

  val terminatedExactlyOnce = endNotices.length() == endNoticeSet.size()
  val terminatedOnlyAfterEnd = terminatedSet.forall(p => not(registered.contains(p._2)))
  val noForwardAfterRemoval = runLog.foldl(true, (ok, entry) => ok and endLog.foldl(true, (ok2, fin) => ok2 and (fin._2 != entry._2 or fin._1 > entry._1)))
  val watchSurvivesUntilUnwatchOrSelfStop = mustStayWatching.forall(p => watching.contains(p))
  val noOrphanRefs = registered.forall(r => aliveActors.contains(parentOfRef(parentOf, r)))
  val invariant =
    terminatedExactlyOnce and terminatedOnlyAfterEnd and noForwardAfterRemoval
      and watchSurvivesUntilUnwatchOrSelfStop and noOrphanRefs

  run removalNotifiesWatchersExactlyOnceTest =
    init
      .then(addRef(2))
      .then(watchByActor(0))
      .then(removeRef(0))
      .expect(terminatedList == [(Probe, 0)] and registered == Set())

  run watchingSurvivesTheNoticeUntilUnwatchTest =
    init
      .then(addRef(2))
      .then(addRef(2))
      .then(watchByRef(0, 1))
      .then(removeRef(1))
      .expect(terminatedList == [(0, 1)] and watching == Set((0, 1)))
      .then(unwatchByRef(0, 1))
      .expect(watching == Set())

  run parentStopEndsRefsThroughTheChainTest =
    init
      .then(addRef(2))
      .then(watchByActor(0))
      .then(stopActor(1))
      .expect(registered == Set() and terminatedList == [(Probe, 0)] and aliveActors == Set())

  run bugDoubleCleanupTest =
    init
      .then(addRef(2))
      .then(watchByActor(0))
      .then(removeRefBugDoubleNotify(0))
      .expect(not(terminatedExactlyOnce))

  run bugAutoUnwatchTest =
    init
      .then(addRef(2))
      .then(addRef(2))
      .then(watchByRef(0, 1))
      .then(removeRefBugAutoUnwatch(1))
      .expect(not(watchSurvivesUntilUnwatchOrSelfStop))

  run bugOrphanRefsTest =
    init
      .then(addRef(2))
      .then(stopActorBugOrphan(2))
      .expect(not(noOrphanRefs))

  run sendToRunsOnlyWhileRegisteredTest =
    init
      .then(addRef(2))
      .then(sendTo(0))
      .expect(forwarded == 1 and deadMessages == 0)
      .then(removeRef(0))
      .then(sendTo(0))
      .expect(forwarded == 1 and deadMessages == 1 and noForwardAfterRemoval)

  run bugSendToEndedRefTest =
    init
      .then(addRef(2))
      .then(removeRef(0))
      .then(sendToBugRunsEnded(0))
      .expect(not(noForwardAfterRemoval))

  run parentStopSkipsWatchersThatAlreadyEndedTest =
    init
      .then(addRef(2))
      .then(addRef(2))
      .then(watchByRef(0, 1))
      .then(stopActor(2))
      .expect(terminatedList == [] and registered == Set() and aliveActors == Set(1))

  run parentStopNotifiesWatchersThatEndLaterTest =
    init
      .then(addRef(2))
      .then(addRef(2))
      .then(watchByRef(1, 0))
      .then(stopActor(2))
      .expect(terminatedList == [(1, 0)] and registered == Set())
}
