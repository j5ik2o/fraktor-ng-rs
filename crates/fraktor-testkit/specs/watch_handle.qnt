// Coroner WatchHandle (crates/fraktor-testkit/src/coroner/watch_handle.rs and
// src/coroner.rs).
//
// Two threads share one outcome. The test thread cancels; the Coroner thread
// starts, waits, and expires if no cancel came, then writes the report and
// finishes. The first of cancel and expire decides the outcome.
module watch_handle {
  type Outcome = Pending | Cancelled | Expired

  var outcome: Outcome
  // The first decided outcome, kept to check that it never changes.
  var decided: Outcome
  var started: bool
  var reportWritten: bool
  var finished: bool
  // Whether the test thread's cancel has returned, and what result() said after.
  var cancelReturned: bool

  action init = all {
    outcome' = Pending,
    decided' = Pending,
    started' = false,
    reportWritten' = false,
    finished' = false,
    cancelReturned' = false,
  }

  pure def decide(current: Outcome, wanted: Outcome): Outcome =
    match current {
      | Pending => wanted
      | Cancelled => Cancelled
      | Expired => Expired
    }

  action start = all {
    not(started),
    started' = true,
    outcome' = outcome,
    decided' = decided,
    reportWritten' = reportWritten,
    finished' = finished,
    cancelReturned' = cancelReturned,
  }

  // cancel: outcome.get_or_insert(true), then wait for finished (or the
  // start-and-stop timeout, so cancel may return before finished).
  action cancel = all {
    not(cancelReturned),
    outcome' = decide(outcome, Cancelled),
    decided' = decide(decided, Cancelled),
    cancelReturned' = true,
    started' = started,
    reportWritten' = reportWritten,
    finished' = finished,
  }

  // expire_unless_cancelled: only a Pending outcome becomes Expired.
  action expire = all {
    started,
    not(finished),
    outcome == Pending,
    outcome' = Expired,
    decided' = decide(decided, Expired),
    started' = started,
    reportWritten' = reportWritten,
    finished' = finished,
    cancelReturned' = cancelReturned,
  }

  action writeReport = all {
    outcome == Expired,
    not(reportWritten),
    not(finished),
    reportWritten' = true,
    outcome' = outcome,
    decided' = decided,
    started' = started,
    finished' = finished,
    cancelReturned' = cancelReturned,
  }

  // The thread finishes once it saw a cancel, or after it wrote the report.
  action finish = all {
    started,
    not(finished),
    outcome == Cancelled or reportWritten,
    finished' = true,
    outcome' = outcome,
    decided' = decided,
    started' = started,
    reportWritten' = reportWritten,
    cancelReturned' = cancelReturned,
  }

  action step = any { start, cancel, expire, writeReport, finish }

  val outcomeNeverChanges = decided == Pending or outcome == decided
  val reportOnlyWhenExpired = reportWritten implies outcome == Expired
  val cancelledWritesNothing = outcome == Cancelled implies not(reportWritten)
  val finishedConsistent = finished implies (reportWritten iff outcome == Expired)
  val resultAfterCancelIsDecided = cancelReturned implies outcome != Pending
  val invariant = outcomeNeverChanges and reportOnlyWhenExpired and cancelledWritesNothing
    and finishedConsistent and resultAfterCancelIsDecided

  run cancelBeforeExpiryTest =
    init.then(start).then(cancel).then(finish)
      .expect(outcome == Cancelled and not(reportWritten) and finished)

  run expiryBeforeCancelTest =
    init.then(start).then(expire).then(writeReport).then(finish).then(cancel)
      .expect(outcome == Expired and reportWritten and finished and cancelReturned)

  run noExpiryAfterCancelTest =
    init.then(start).then(cancel).then(expire).fail()
}
