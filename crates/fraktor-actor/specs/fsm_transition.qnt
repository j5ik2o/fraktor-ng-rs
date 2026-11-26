// FSM transition listeners (crates/fraktor-actor/src/kernel/actor/fsm.rs).
//
// The model of the listener contract that issue #194 ports from Pekko's
// FSMTransitionSpec (design docs/design/issue-194-specs.md sections 3
// and 5). Two listeners (1 and 2) are enough: the properties are about
// membership in the listener set, not about counts above two.
//
// `stopListener` and `processNotice` are separate actions on purpose:
// between the stop and the processing of its notice a transition may
// still be sent to the stopped listener, where it goes to dead letters.
// That delivery is not a live delivery (F5, restated by the design
// review M3), so `liveNotified` excludes listeners whose stop is
// pending, while `sentToStopped` records that the send happened.
//
// Subscribing a listener that has already stopped enqueues its notice
// at once (the port's `watch` does), so `subscribe` of a gone listener
// adds it to the pending set instead of the listener set.
module fsm_transition {
  var state: int
  var listeners: Set[int]
  var pending: Set[int]
  var gone: Set[int]
  var lastFrom: int
  var lastTo: int
  var liveNotified: Set[int]
  var sentToStopped: Set[int]
  var currentSent: Set[int]

  pure val allListeners = Set(1, 2)

  action init = all {
    state' = 0,
    listeners' = Set(),
    pending' = Set(),
    gone' = Set(),
    lastFrom' = 0,
    lastTo' = 0,
    liveNotified' = Set(),
    sentToStopped' = Set(),
    currentSent' = Set(),
  }

  action subscribe(l: int): bool = all {
    if (gone.contains(l)) all {
      // Already stopped: the notice is enqueued at once and the
      // listener never enters the set.
      listeners' = listeners,
      pending' = pending.union(Set(l)),
      currentSent' = currentSent,
    } else all {
      listeners' = listeners.union(Set(l)),
      pending' = pending,
      currentSent' = currentSent.union(Set(l)),
    },
    state' = state,
    gone' = gone,
    lastFrom' = lastFrom,
    lastTo' = lastTo,
    liveNotified' = liveNotified,
    sentToStopped' = sentToStopped,
  }

  action unsubscribe(l: int): bool = all {
    listeners' = listeners.exclude(Set(l)),
    state' = state,
    pending' = pending,
    gone' = gone,
    lastFrom' = lastFrom,
    lastTo' = lastTo,
    liveNotified' = liveNotified,
    sentToStopped' = sentToStopped,
    currentSent' = currentSent,
  }

  action stopListener(l: int): bool = all {
    listeners.contains(l),
    pending' = pending.union(Set(l)),
    state' = state,
    listeners' = listeners,
    gone' = gone,
    lastFrom' = lastFrom,
    lastTo' = lastTo,
    liveNotified' = liveNotified,
    sentToStopped' = sentToStopped,
    currentSent' = currentSent,
  }

  action processNotice = all {
    pending.size() > 0,
    listeners' = listeners.exclude(pending),
    gone' = gone.union(pending),
    pending' = Set(),
    state' = state,
    lastFrom' = lastFrom,
    lastTo' = lastTo,
    liveNotified' = liveNotified,
    sentToStopped' = sentToStopped,
    currentSent' = currentSent,
  }

  // A reported transition (goto, including to the same state): every
  // listener in the set is sent the notice; the ones whose stop is
  // pending receive it as a dead letter, not as a live delivery.
  action tick = all {
    lastFrom' = state,
    lastTo' = 1 - state,
    state' = 1 - state,
    liveNotified' = listeners.exclude(pending),
    sentToStopped' = listeners.intersect(pending),
    listeners' = listeners,
    pending' = pending,
    gone' = gone,
    currentSent' = currentSent,
  }

  // A stay: no notice is sent at all (F1).
  action stay = all {
    liveNotified' = Set(),
    sentToStopped' = Set(),
    state' = state,
    listeners' = listeners,
    pending' = pending,
    gone' = gone,
    lastFrom' = lastFrom,
    lastTo' = lastTo,
    currentSent' = currentSent,
  }

  // The defect that notifies on stay as well.
  action stayBugNotifies = all {
    lastFrom' = state,
    lastTo' = state,
    liveNotified' = listeners.exclude(pending),
    sentToStopped' = listeners.intersect(pending),
    state' = state,
    listeners' = listeners,
    pending' = pending,
    gone' = gone,
    currentSent' = currentSent,
  }

  // The defect that never removes a stopped listener.
  action processNoticeBugKeeps = all {
    pending.size() > 0,
    gone' = gone.union(pending),
    pending' = Set(),
    listeners' = listeners,
    state' = state,
    lastFrom' = lastFrom,
    lastTo' = lastTo,
    liveNotified' = liveNotified,
    sentToStopped' = sentToStopped,
    currentSent' = currentSent,
  }

  action step = any {
    init,
    subscribe(1), subscribe(2),
    unsubscribe(1), unsubscribe(2),
    stopListener(1), stopListener(2),
    processNotice, tick, stay,
  }

  // F2/F4: a reported transition reaches exactly the live listeners.
  val notifiedAreLiveListeners = liveNotified == listeners.exclude(pending)
  // F5 (restated): once the notice is processed the stopped listener
  // is out of the set, and a stopped listener never counts as live.
  val stoppedAreOutAfterNotice = gone.intersect(listeners) == Set() or pending != Set()
  val noLiveDeliveryToStopped = liveNotified.intersect(gone) == Set()
  // F4: a subscribe is answered with the current state.
  val subscribeAnswersCurrent = listeners.subsetOf(currentSent)

  val invariant = notifiedAreLiveListeners
    and stoppedAreOutAfterNotice
    and noLiveDeliveryToStopped
    and subscribeAnswersCurrent

  run stopThenTickThenProcessTest =
    init
      .then(subscribe(1))
      .then(stopListener(1))
      .then(tick)
      .expect(liveNotified == Set() and sentToStopped == Set(1))
      .then(processNotice)
      .expect(listeners == Set() and gone == Set(1))

  run stayDoesNotNotifyTest =
    init
      .then(subscribe(1))
      .then(stay)
      .expect(liveNotified == Set() and lastFrom == 0 and lastTo == 0)

  run subscribeStoppedListenerTest =
    init
      .then(subscribe(1))
      .then(stopListener(1))
      .then(processNotice)
      .then(subscribe(1))
      .expect(listeners == Set() and pending == Set(1))

  run bugStayNotifiesTest =
    init
      .then(subscribe(1))
      .then(stayBugNotifies)
      .expect(not(liveNotified == Set()))

  run bugKeepsStoppedListenerTest =
    init
      .then(subscribe(1))
      .then(stopListener(1))
      .then(processNoticeBugKeeps)
      .expect(not(gone.intersect(listeners) == Set()))
}
