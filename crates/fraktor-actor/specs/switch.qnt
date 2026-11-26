// Switch (crates/fraktor-actor/src/kernel/switch/switch_value.rs).
//
// One atomic state and one lock. Every state change happens while the
// changer holds the lock: the locking operations acquire it first, and the
// reentrant changes the original's monitor allows go through the
// LockedSwitch window, which is a change by the holder. An action that
// fails (or panics) reverts the state through a drop guard while the lock
// is still held.
//
// The history variables `onAtLock` and `lastChanger` exist so that S5 of
// the design (docs/design/issue-186-util-specs.md §3 and §4) can be an
// invariant at all: while a thread is inside `locked`, the state either
// still equals the state at entry, or the last change was made by that
// same thread. `actingFrom` records the state before the flip of an
// action that is still running, so the invariant can say the state is the
// flipped one while the action runs.
module switch {
  type Tid = TA | TB

  var on: bool
  var hasOwner: bool
  var owner: Tid
  var acting: bool
  var actingFrom: bool
  var onAtLock: bool
  var lastChanger: Tid

  action init = {
    nondet startOn = Set(true, false).oneOf()
    all {
      on' = startOn,
      hasOwner' = false,
      owner' = TA,
      acting' = false,
      actingFrom' = startOn,
      onAtLock' = startOn,
      lastChanger' = TA,
    }
  }

  action acquire(t: Tid): bool = all {
    not(hasOwner),
    hasOwner' = true,
    owner' = t,
    onAtLock' = on,
    on' = on,
    acting' = acting,
    actingFrom' = actingFrom,
    lastChanger' = lastChanger,
  }

  action release(t: Tid): bool = all {
    hasOwner and owner == t and not(acting),
    hasOwner' = false,
    owner' = owner,
    onAtLock' = onAtLock,
    on' = on,
    acting' = acting,
    actingFrom' = actingFrom,
    lastChanger' = lastChanger,
  }

  // A change by the holder (a locking operation that acquired first, or a
  // change through the LockedSwitch window): CAS the state, then the
  // action runs while the state is flipped.
  action flipAndAct(t: Tid): bool = all {
    hasOwner and owner == t and not(acting),
    on' = not(on),
    acting' = true,
    actingFrom' = on,
    lastChanger' = t,
    hasOwner' = hasOwner,
    owner' = owner,
    onAtLock' = onAtLock,
  }

  action actionSucceeds(t: Tid): bool = all {
    hasOwner and owner == t and acting,
    acting' = false,
    on' = on,
    hasOwner' = hasOwner,
    owner' = owner,
    actingFrom' = actingFrom,
    onAtLock' = onAtLock,
    lastChanger' = lastChanger,
  }

  // The drop guard reverts the flip with a compare-exchange, still under
  // the lock.
  action actionFails(t: Tid): bool = all {
    hasOwner and owner == t and acting,
    acting' = false,
    on' = actingFrom,
    hasOwner' = hasOwner,
    owner' = owner,
    actingFrom' = actingFrom,
    onAtLock' = onAtLock,
    lastChanger' = lastChanger,
  }

  action step = any {
    acquire(TA), acquire(TB),
    release(TA), release(TB),
    flipAndAct(TA), flipAndAct(TB),
    actionSucceeds(TA), actionSucceeds(TB),
    actionFails(TA), actionFails(TB),
  }

  // While an action runs, the state is the flipped one and the holder is
  // the thread running it.
  val actingMeansFlippedByOwner = acting implies (on != actingFrom and hasOwner and lastChanger == owner)
  // S5 as an invariant, through the history variables: while a thread
  // holds the lock, no other thread has changed the state since entry.
  val noForeignChangeWhileLocked = hasOwner implies (on == onAtLock or lastChanger == owner)
  val invariant = actingMeansFlippedByOwner and noForeignChangeWhileLocked

  action initOff = all {
    on' = false,
    hasOwner' = false,
    owner' = TA,
    acting' = false,
    actingFrom' = false,
    onAtLock' = false,
    lastChanger' = TA,
  }

  run switchOnWithSucceedingActionTest =
    initOff
      .then(acquire(TA))
      .then(flipAndAct(TA))
      .then(actionSucceeds(TA))
      .then(release(TA))
      .expect(on)

  run failingActionRevertsTest =
    initOff
      .then(acquire(TA))
      .then(flipAndAct(TA))
      .then(actionFails(TA))
      .then(release(TA))
      .expect(not(on))

  run otherThreadChangesAfterLockedSectionTest =
    initOff
      .then(acquire(TA))
      .then(flipAndAct(TA))
      .then(actionSucceeds(TA))
      .then(all { assert(on and hasOwner and owner == TA), release(TA) })
      .then(acquire(TB))
      .then(flipAndAct(TB))
      .then(actionSucceeds(TB))
      .then(release(TB))
      .expect(not(on))

  run reentryThroughTheWindowTest =
    initOff
      .then(acquire(TA))
      .then(flipAndAct(TA))
      .then(actionFails(TA))
      .then(flipAndAct(TA))
      .then(actionSucceeds(TA))
      .then(release(TA))
      .expect(on)
}
