// DeadLetterListener (crates/fraktor-actor/src/kernel/event/dead_letter_listener.rs).
//
// The counting state machine that logs dead letters up to a limit, then
// suspends logging for a duration, then resumes on the first event after
// the deadline and resets its counter. Time is a step counter; the limit
// and the suspend duration are state variables set by initWith, because a
// `const` left uninitialized makes the CI check fail with QNT500. The
// limit -1 stands for Unlimited. Suppressed events (a SuppressedDeadLetter,
// or a payload the system registered) never reach the counting actions:
// receiveSuppressed models their arrival and changes nothing but the
// ghost of what arrived last.
module dead_letter_listener {
  type Mode = Logging | Suspended
  type Kind = DeadLetterKind | DroppedKind | UnhandledKind | SuppressedKind

  var mode: Mode
  var count: int
  var now: int
  var deadline: int
  var limit: int
  var duration: int
  // Ghosts the invariants read: unsuppressed events received since the
  // last reset, log lines emitted, the count the last log line carried,
  // the log total at the moment of suspension, and the last resume.
  var received: int
  var base: int
  var logged: int
  var lastLogCount: int
  var loggedAtSuspend: int
  var lastKind: Kind
  var lastNotLogged: int
  var resumeNow: int
  var resumeDeadline: int
  var justResumed: bool

  action initWith(l: int, d: int): bool = all {
    limit' = l,
    duration' = d,
    mode' = Logging,
    count' = 0,
    now' = 0,
    deadline' = 0,
    received' = 0,
    base' = 0,
    logged' = 0,
    lastLogCount' = -1,
    loggedAtSuspend' = -1,
    lastKind' = SuppressedKind,
    lastNotLogged' = -1,
    resumeNow' = -2,
    resumeDeadline' = -2,
    justResumed' = false,
  }

  action init = {
    nondet l = Set(-1, 1, 2, 4).oneOf()
    nondet d = Set(0, 2, 5).oneOf()
    initWith(l, d)
  }

  // One unsuppressed dead letter of the given kind arrives. The three
  // counted kinds share the counting, so one action serves them; the kind
  // is kept only as a ghost.
  action receiveCounted(kind: Kind): bool = all {
    lastKind' = kind,
    limit' = limit,
    duration' = duration,
    now' = now,
    if (mode == Logging) all {
      received' = received + 1,
      logged' = logged + 1,
      lastLogCount' = count + 1,
      justResumed' = false,
      if (limit >= 1 and count + 1 == limit) all {
        mode' = Suspended,
        count' = count + 1,
        deadline' = now + duration,
        loggedAtSuspend' = logged + 1,
        base' = base,
        lastNotLogged' = lastNotLogged,
        resumeNow' = resumeNow,
        resumeDeadline' = resumeDeadline,
      } else all {
        mode' = Logging,
        count' = count + 1,
        deadline' = deadline,
        loggedAtSuspend' = loggedAtSuspend,
        base' = base,
        lastNotLogged' = lastNotLogged,
        resumeNow' = resumeNow,
        resumeDeadline' = resumeDeadline,
      },
    } else if (now > deadline) all {
      // Resume: this event is logged with the pre-reset count, then the
      // counter resets.
      received' = received + 1,
      logged' = logged + 1,
      lastLogCount' = count + 1,
      mode' = Logging,
      count' = 0,
      deadline' = deadline,
      loggedAtSuspend' = loggedAtSuspend,
      resumeNow' = now,
      resumeDeadline' = deadline,
      justResumed' = true,
      lastNotLogged' = count - limit,
      base' = received + 1,
    } else all {
      received' = received + 1,
      logged' = logged,
      lastLogCount' = lastLogCount,
      mode' = Suspended,
      count' = count + 1,
      deadline' = deadline,
      loggedAtSuspend' = loggedAtSuspend,
      lastNotLogged' = lastNotLogged,
      resumeNow' = resumeNow,
      resumeDeadline' = resumeDeadline,
      justResumed' = false,
      base' = base,
    },
  }

  action receiveDeadLetter = receiveCounted(DeadLetterKind)
  action receiveDropped = receiveCounted(DroppedKind)
  action receiveUnhandled = receiveCounted(UnhandledKind)

  // A suppressed event arrives: it is neither counted nor logged.
  action receiveSuppressed = all {
    lastKind' = SuppressedKind,
    mode' = mode, count' = count, now' = now, deadline' = deadline,
    limit' = limit, duration' = duration,
    received' = received, logged' = logged, lastLogCount' = lastLogCount,
    loggedAtSuspend' = loggedAtSuspend,
    lastNotLogged' = lastNotLogged,
    resumeNow' = resumeNow, resumeDeadline' = resumeDeadline,
    justResumed' = false,
    base' = base,
  }

  action tick = all {
    now < 12,
    now' = now + 1,
    mode' = mode, count' = count, deadline' = deadline,
    limit' = limit, duration' = duration,
    received' = received, logged' = logged, lastLogCount' = lastLogCount,
    loggedAtSuspend' = loggedAtSuspend, lastKind' = lastKind,
    lastNotLogged' = lastNotLogged,
    resumeNow' = resumeNow, resumeDeadline' = resumeDeadline,
    justResumed' = false,
    base' = base,
  }

  action step = any {
    receiveDeadLetter, receiveDropped, receiveUnhandled, receiveSuppressed, tick,
  }

  // L1: the count is exactly the unsuppressed events received since the
  // last reset; `base` is the received total at the last reset, and a
  // suppressed event moves neither side.
  val countIsUnsuppressedSeen = count == received - base
  // L2: while suspended, no log line is emitted after the suspending one.
  val noLogWhileSuspended = mode == Suspended implies logged == loggedAtSuspend
  // L3: suspension is entered exactly at the limit, with that count logged.
  val suspendAtLimit = mode == Suspended implies (limit >= 1 and lastLogCount == limit)
  // L4: a resume leaves the counter at zero in Logging mode.
  val resumeResetsCount = justResumed implies (count == 0 and mode == Logging)
  // L5: a resume happens only strictly after the deadline.
  val deadlineIsStrict = resumeNow == -2 or resumeNow > resumeDeadline
  // L6: the resume line's not-logged count is the pre-reset count minus
  // the limit minus the resume event itself.
  val resumeNotLoggedIsExact = justResumed implies lastNotLogged == lastLogCount - limit - 1

  val invariant = countIsUnsuppressedSeen and noLogWhileSuspended and suspendAtLimit
    and resumeResetsCount and deadlineIsStrict and resumeNotLoggedIsExact

  // The original settings: limit 4, suspend duration 2. The event at the
  // deadline itself does not resume; the first one after it does, with
  // 6 - 4 - 1 = 1 event not logged.
  run limit4Test =
    initWith(4, 2)
      .then(receiveDeadLetter)
      .then(receiveDropped)
      .then(receiveUnhandled)
      .then(all { assert(mode == Logging and count == 3 and logged == 3), receiveDeadLetter })
      .then(all {
        assert(mode == Suspended and count == 4 and logged == 4 and deadline == 2),
        tick,
      })
      .then(tick)
      .then(all { assert(now == 2 and mode == Suspended), receiveDeadLetter })
      .then(all { assert(mode == Suspended and count == 5 and logged == 4), tick })
      .then(receiveDeadLetter)
      .expect(mode == Logging and count == 0 and logged == 5 and lastLogCount == 6 and lastNotLogged == 1)

  // Limit 1: the first event already suspends.
  run limit1Test =
    initWith(1, 2)
      .then(receiveDeadLetter)
      .expect(mode == Suspended and count == 1 and logged == 1 and lastLogCount == 1)

  // Limit 2, duration 0: the deadline is the current step, so events at
  // the same step never resume; one tick later the first event resumes,
  // with 4 - 2 - 1 = 1 event not logged.
  run limit2shortTest =
    initWith(2, 0)
      .then(receiveDeadLetter)
      .then(receiveDeadLetter)
      .then(all { assert(mode == Suspended and deadline == 0), receiveDeadLetter })
      .then(all { assert(mode == Suspended and count == 3 and logged == 2), tick })
      .then(receiveDeadLetter)
      .expect(mode == Logging and count == 0 and logged == 3 and lastLogCount == 4 and lastNotLogged == 1)

  // Unlimited: logging never suspends.
  run unlimitedTest =
    initWith(-1, 2)
      .then(receiveDeadLetter)
      .then(receiveDropped)
      .then(receiveSuppressed)
      .then(receiveUnhandled)
      .then(receiveDeadLetter)
      .expect(mode == Logging and count == 4 and logged == 4)
}
