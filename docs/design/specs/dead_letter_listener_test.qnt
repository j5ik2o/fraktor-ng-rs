// Negative controls for dead_letter_listener.qnt.
//
// Each `bug*` module is the listener model of
// crates/fraktor-actor/specs/dead_letter_listener.qnt with one deliberate
// defect, reduced to the counted-kind arrivals (the three counted kinds
// share the counting, so one arrival action serves them). Each run drives
// the module to the state where its defect shows and asserts that the
// invariant it breaks does NOT hold:
//
//   quint test --main=bugNoReset --match=noResetBreaksResumeTest docs/design/specs/dead_letter_listener_test.qnt
//   quint test --main=bugCountsSuppressed --match=countsSuppressedBreaksCountTest docs/design/specs/dead_letter_listener_test.qnt
//   quint test --main=bugLogsWhileSuspended --match=logsWhileSuspendedBreaksNoLogTest docs/design/specs/dead_letter_listener_test.qnt
//   quint test --main=bugResumeAtDeadline --match=resumeAtDeadlineBreaksStrictTest docs/design/specs/dead_letter_listener_test.qnt
//   quint test --main=bugOffByOneNotLogged --match=offByOneBreaksNotLoggedTest docs/design/specs/dead_letter_listener_test.qnt
//
// `quint verify` is not used: it needs Apalache, which the project keeps
// out of CI (docs/design/quint-in-ci.md).

// Defect: a resume forgets to reset the counter.
module bugNoReset {
  type Mode = Logging | Suspended

  var mode: Mode
  var count: int
  var now: int
  var deadline: int
  var received: int
  var base: int
  var justResumed: bool

  action initWith(l: int, d: int): bool = all {
    mode' = Logging, count' = 0, now' = 0, deadline' = 0,
    received' = 0, base' = 0, justResumed' = false,
    // The limit and duration are fixed by the run through the trace
    // length; this reduced model fixes limit 2 and duration 1 in the
    // actions below.
  }

  action init = initWith(2, 1)

  action receive = all {
    now' = now,
    received' = received + 1,
    if (mode == Logging) all {
      justResumed' = false,
      base' = base,
      if (count + 1 == 2) all {
        mode' = Suspended, count' = count + 1, deadline' = now + 1,
      } else all {
        mode' = Logging, count' = count + 1, deadline' = deadline,
      },
    } else if (now > deadline) all {
      // BUG: count keeps its value instead of resetting to 0.
      mode' = Logging, count' = count + 1, deadline' = deadline,
      base' = received + 1, justResumed' = true,
    } else all {
      mode' = Suspended, count' = count + 1, deadline' = deadline,
      base' = base, justResumed' = false,
    },
  }

  action tick = all {
    now' = now + 1,
    mode' = mode, count' = count, deadline' = deadline,
    received' = received, base' = base, justResumed' = false,
  }

  val countIsUnsuppressedSeen = count == received - base
  val resumeResetsCount = justResumed implies (count == 0 and mode == Logging)
  val invariant = countIsUnsuppressedSeen and resumeResetsCount

  run noResetBreaksResumeTest =
    init
      .then(receive)
      .then(receive)
      .then(tick)
      .then(tick)
      .then(receive)
      .expect(not(resumeResetsCount))
}

// Defect: suppressed events are counted like the others.
module bugCountsSuppressed {
  type Mode = Logging | Suspended

  var mode: Mode
  var count: int
  var received: int
  var base: int

  action init = all {
    mode' = Logging, count' = 0, received' = 0, base' = 0,
  }

  action receiveCounted = all {
    mode' = mode, count' = count + 1, received' = received + 1, base' = base,
  }

  // BUG: a suppressed arrival goes through the counting action.
  action receiveSuppressed = all {
    mode' = mode, count' = count + 1, received' = received, base' = base,
  }

  val countIsUnsuppressedSeen = count == received - base
  val invariant = countIsUnsuppressedSeen

  run countsSuppressedBreaksCountTest =
    init.then(receiveSuppressed).expect(not(countIsUnsuppressedSeen))
}

// Defect: events are logged while suspended, before the deadline.
module bugLogsWhileSuspended {
  type Mode = Logging | Suspended

  var mode: Mode
  var count: int
  var now: int
  var deadline: int
  var logged: int
  var loggedAtSuspend: int

  action init = all {
    mode' = Logging, count' = 0, now' = 0, deadline' = 0, logged' = 0, loggedAtSuspend' = -1,
  }

  action receive = all {
    now' = now,
    if (mode == Logging) all {
      logged' = logged + 1,
      if (count + 1 == 2) all {
        mode' = Suspended, count' = count + 1, deadline' = now + 2, loggedAtSuspend' = logged + 1,
      } else all {
        mode' = Logging, count' = count + 1, deadline' = deadline, loggedAtSuspend' = loggedAtSuspend,
      },
    } else all {
      // BUG: the suspended branch logs the event.
      mode' = Suspended, count' = count + 1, deadline' = deadline,
      logged' = logged + 1, loggedAtSuspend' = loggedAtSuspend,
    },
  }

  val noLogWhileSuspended = mode == Suspended implies logged == loggedAtSuspend
  val invariant = noLogWhileSuspended

  run logsWhileSuspendedBreaksNoLogTest =
    init.then(receive).then(receive).then(receive).expect(not(noLogWhileSuspended))
}

// Defect: a resume happens at the deadline itself, not strictly after it.
module bugResumeAtDeadline {
  type Mode = Logging | Suspended

  var mode: Mode
  var count: int
  var now: int
  var deadline: int
  var resumeNow: int
  var resumeDeadline: int

  action init = all {
    mode' = Logging, count' = 0, now' = 0, deadline' = 0, resumeNow' = -2, resumeDeadline' = -2,
  }

  action receive = all {
    now' = now,
    if (mode == Logging) all {
      resumeNow' = resumeNow, resumeDeadline' = resumeDeadline,
      if (count + 1 == 2) all {
        mode' = Suspended, count' = count + 1, deadline' = now + 2,
      } else all {
        mode' = Logging, count' = count + 1, deadline' = deadline,
      },
      // BUG: >= instead of >.
    } else if (now >= deadline) all {
      mode' = Logging, count' = 0, deadline' = deadline,
      resumeNow' = now, resumeDeadline' = deadline,
    } else all {
      mode' = Suspended, count' = count + 1, deadline' = deadline,
      resumeNow' = resumeNow, resumeDeadline' = resumeDeadline,
    },
  }

  action tick = all {
    now' = now + 1,
    mode' = mode, count' = count, deadline' = deadline,
    resumeNow' = resumeNow, resumeDeadline' = resumeDeadline,
  }

  val deadlineIsStrict = resumeNow == -2 or resumeNow > resumeDeadline
  val invariant = deadlineIsStrict

  run resumeAtDeadlineBreaksStrictTest =
    init
      .then(receive)
      .then(receive)
      .then(tick)
      .then(tick)
      .then(all { assert(now == 2 and mode == Suspended), receive })
      .expect(not(deadlineIsStrict))
}

// Defect: the "not logged" figure of the resume line is one too large.
module bugOffByOneNotLogged {
  type Mode = Logging | Suspended

  var mode: Mode
  var count: int
  var now: int
  var deadline: int
  var lastNotLogged: int

  action init = all {
    mode' = Logging, count' = 0, now' = 0, deadline' = 0, lastNotLogged' = -1,
  }

  action receive = all {
    now' = now,
    if (mode == Logging) all {
      lastNotLogged' = lastNotLogged,
      if (count + 1 == 2) all {
        mode' = Suspended, count' = count + 1, deadline' = now + 1,
      } else all {
        mode' = Logging, count' = count + 1, deadline' = deadline,
      },
    } else if (now > deadline) all {
      // BUG: count - limit instead of count - limit - 1.
      mode' = Logging, lastNotLogged' = (count + 1) - 2, count' = 0, deadline' = deadline,
    } else all {
      mode' = Suspended, count' = count + 1, deadline' = deadline, lastNotLogged' = lastNotLogged,
    },
  }

  action tick = all {
    now' = now + 1,
    mode' = mode, count' = count, deadline' = deadline, lastNotLogged' = lastNotLogged,
  }

  // The resume line must report count - limit - 1 = 4 - 2 - 1 = 1 for the
  // trace of the run: two logged, two counted while suspended, resume on
  // the fifth.
  val notLoggedIsExact = lastNotLogged == -1 or lastNotLogged == 1
  val invariant = notLoggedIsExact

  run offByOneBreaksNotLoggedTest =
    init
      .then(receive)
      .then(receive)
      .then(receive)
      .then(tick)
      .then(tick)
      .then(receive)
      .expect(not(notLoggedIsExact))
}
