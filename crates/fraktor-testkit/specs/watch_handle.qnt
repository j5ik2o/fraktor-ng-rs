// Coroner WatchHandle (crates/fraktor-testkit/src/coroner/watch_handle.rs and
// src/coroner.rs).
//
// The test thread cancels (any number of times: the handle is Clone). A cancel
// first decides the outcome under the lock (Cancelled unless already decided),
// then waits for the Coroner thread to finish, or gives up after the
// start-and-stop duration. The Coroner thread starts, waits, expires if the
// outcome is still Pending (decided in the same lock), tries to write the
// report (the output may fail part way), and finishes.
module watch_handle {
  type Outcome = Pending | Cancelled | Expired
  type Report = NotTried | Written | Failed
  type CancelCall = NoCall | Requested | ReturnedFinished | ReturnedTimedOut

  var outcome: Outcome
  var decided: Outcome
  var started: bool
  var report: Report
  var finished: bool
  var cancelCall: CancelCall
  var cancels: int

  action init = all {
    outcome' = Pending, decided' = Pending, started' = false, report' = NotTried,
    finished' = false, cancelCall' = NoCall, cancels' = 0,
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
    outcome' = outcome, decided' = decided, report' = report, finished' = finished,
    cancelCall' = cancelCall, cancels' = cancels,
  }

  // outcome.get_or_insert(true) and notify, under the lock.
  action cancelRequest = all {
    cancelCall != Requested,
    cancels < 3,
    outcome' = decide(outcome, Cancelled),
    decided' = decide(decided, Cancelled),
    cancelCall' = Requested,
    cancels' = cancels + 1,
    started' = started, report' = report, finished' = finished,
  }

  // wait_timeout_while(!finished): returns once finished ...
  action cancelReturnFinished = all {
    cancelCall == Requested,
    finished,
    cancelCall' = ReturnedFinished,
    outcome' = outcome, decided' = decided, started' = started, report' = report,
    finished' = finished, cancels' = cancels,
  }

  // ... or when the start-and-stop duration passes first.
  action cancelReturnTimedOut = all {
    cancelCall == Requested,
    not(finished),
    cancelCall' = ReturnedTimedOut,
    outcome' = outcome, decided' = decided, started' = started, report' = report,
    finished' = finished, cancels' = cancels,
  }

  // expire_unless_cancelled: only a Pending outcome becomes Expired.
  action expire = all {
    started,
    not(finished),
    outcome == Pending,
    outcome' = Expired,
    decided' = decide(decided, Expired),
    started' = started, report' = report, finished' = finished,
    cancelCall' = cancelCall, cancels' = cancels,
  }

  action writeReport(result: Report): bool = all {
    outcome == Expired,
    report == NotTried,
    not(finished),
    report' = result,
    outcome' = outcome, decided' = decided, started' = started, finished' = finished,
    cancelCall' = cancelCall, cancels' = cancels,
  }

  // The thread finishes once it saw a cancel, or after it tried the report.
  action finish = all {
    started,
    not(finished),
    outcome == Cancelled or report != NotTried,
    finished' = true,
    outcome' = outcome, decided' = decided, started' = started, report' = report,
    cancelCall' = cancelCall, cancels' = cancels,
  }

  action step = any {
    start, cancelRequest, cancelReturnFinished, cancelReturnTimedOut, expire,
    writeReport(Written), writeReport(Failed), finish,
  }

  val outcomeNeverChanges = decided == Pending or outcome == decided
  val reportOnlyWhenExpired = report != NotTried implies outcome == Expired
  val cancelledWritesNothing = outcome == Cancelled implies report == NotTried
  val finishedConsistent = finished implies (report != NotTried iff outcome == Expired)
  val cancelDecides = cancels > 0 implies outcome != Pending
  val cancelWaitsForFinish = cancelCall == ReturnedFinished implies finished
  val invariant = outcomeNeverChanges and reportOnlyWhenExpired and cancelledWritesNothing
    and finishedConsistent and cancelDecides and cancelWaitsForFinish

  run cancelBeforeExpiryTest =
    init.then(start).then(cancelRequest).then(finish).then(cancelReturnFinished)
      .expect(outcome == Cancelled and report == NotTried and finished)

  run expiryBeforeCancelTest =
    init.then(start).then(expire).then(writeReport(Written)).then(finish)
      .then(cancelRequest).then(cancelReturnFinished)
      .expect(outcome == Expired and report == Written and finished)

  run cancelCannotReturnFinishedBeforeFinishTest =
    init.then(start).then(expire).then(cancelRequest).then(cancelReturnFinished).fail()

  run failedOutputStillFinishesTest =
    init.then(start).then(expire).then(writeReport(Failed)).then(finish)
      .expect(outcome == Expired and report == Failed and finished)

  run noExpiryAfterCancelTest =
    init.then(start).then(cancelRequest).then(expire).fail()

  run repeatedCancelKeepsTheOutcomeTest =
    init.then(start).then(cancelRequest).then(cancelReturnTimedOut).then(cancelRequest)
      .expect(outcome == Cancelled and cancels == 2)
}
