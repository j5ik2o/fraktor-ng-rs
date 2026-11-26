// The scheduled-report thread of MetricsKit (crates/fraktor-testkit/src/
// metrics_kit/metrics_kit_value.rs; design docs/design/issue-183-metrics-kit.md
// §2 "レポーターと報告の形" and §6).
//
// Building the kit starts the thread only when the interval is above zero.
// The thread waits on a Condvar paired with the stop flag; on waking it takes
// the lock and either reports (the flag is clear) or leaves (the flag is set).
// shutdown_metrics takes the lock, sets the flag, notifies, releases the lock
// and only then joins the thread; later calls do nothing. Dropping the kit
// runs the same steps unless shutdown_metrics already ran.
module console_reporter {
  type Thread = NotStarted | Waiting | Reporting | StopRequested | Finished
  type Shutdown = NotCalled | Requested | Joining | Returned
  type Lock = Free | Main | Reporter

  var scheduled: bool
  var constructed: bool
  var thread: Thread
  var stopFlag: bool
  var lock: Lock
  var shutdown: Shutdown
  var shutdownCalls: int
  var joins: int
  var dropped: bool
  var reports: int

  action initWith(interval: bool): bool = all {
    scheduled' = interval, constructed' = false, thread' = NotStarted, stopFlag' = false,
    lock' = Free, shutdown' = NotCalled, shutdownCalls' = 0, joins' = 0, dropped' = false,
    reports' = 0,
  }

  action init = {
    nondet interval = Set(true, false).oneOf()
    initWith(interval)
  }

  // MetricsKit::new: spawns the thread only for a positive interval.
  action construct = all {
    not(constructed),
    constructed' = true,
    thread' = if (scheduled) Waiting else NotStarted,
    scheduled' = scheduled, stopFlag' = stopFlag, lock' = lock, shutdown' = shutdown,
    shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped, reports' = reports,
  }

  // wait_timeout returns after the interval with the flag clear: report.
  action wakeToReport = all {
    thread == Waiting,
    lock == Free,
    not(stopFlag),
    reports < 3,
    thread' = Reporting,
    lock' = Reporter,
    reports' = reports + 1,
    scheduled' = scheduled, constructed' = constructed, stopFlag' = stopFlag,
    shutdown' = shutdown, shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped,
  }

  // The report is printed; the thread goes back to waiting.
  action finishReport = all {
    thread == Reporting,
    lock == Reporter,
    thread' = Waiting,
    lock' = Free,
    scheduled' = scheduled, constructed' = constructed, stopFlag' = stopFlag,
    shutdown' = shutdown, shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped,
    reports' = reports,
  }

  // The notify (or any wake) with the flag set: the thread leaves at once,
  // however long the interval is.
  action wakeToStop = all {
    thread == Waiting,
    lock == Free,
    stopFlag,
    thread' = StopRequested,
    lock' = Reporter,
    scheduled' = scheduled, constructed' = constructed, stopFlag' = stopFlag,
    shutdown' = shutdown, shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped,
    reports' = reports,
  }

  action exit = all {
    thread == StopRequested,
    lock == Reporter,
    thread' = Finished,
    lock' = Free,
    scheduled' = scheduled, constructed' = constructed, stopFlag' = stopFlag,
    shutdown' = shutdown, shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped,
    reports' = reports,
  }

  // The first shutdown_metrics, or the drop when shutdown_metrics never ran:
  // take the lock.
  action beginStop(byDrop: bool): bool = all {
    constructed,
    not(dropped),
    shutdown == NotCalled,
    lock == Free,
    lock' = Main,
    shutdown' = Requested,
    shutdownCalls' = if (byDrop) shutdownCalls else shutdownCalls + 1,
    dropped' = byDrop,
    scheduled' = scheduled, constructed' = constructed, thread' = thread, stopFlag' = stopFlag,
    joins' = joins, reports' = reports,
  }

  // Set the flag, notify, and release the lock before joining.
  action requestStop = all {
    shutdown == Requested,
    lock == Main,
    stopFlag' = true,
    lock' = Free,
    shutdown' = Joining,
    scheduled' = scheduled, constructed' = constructed, thread' = thread,
    shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped, reports' = reports,
  }

  // join returns once the thread finished; with no thread there is nothing
  // to join.
  action joinReturn = all {
    shutdown == Joining,
    thread == Finished or thread == NotStarted,
    shutdown' = Returned,
    joins' = if (thread == Finished) joins + 1 else joins,
    scheduled' = scheduled, constructed' = constructed, thread' = thread, stopFlag' = stopFlag,
    lock' = lock, shutdownCalls' = shutdownCalls, dropped' = dropped, reports' = reports,
  }

  // A later shutdown_metrics does nothing.
  action shutdownAgain = all {
    shutdown == Returned,
    not(dropped),
    shutdownCalls < 3,
    shutdownCalls' = shutdownCalls + 1,
    scheduled' = scheduled, constructed' = constructed, thread' = thread, stopFlag' = stopFlag,
    lock' = lock, shutdown' = shutdown, joins' = joins, dropped' = dropped, reports' = reports,
  }

  // The drop after shutdown_metrics returned does nothing more.
  action dropAfterShutdown = all {
    shutdown == Returned,
    not(dropped),
    dropped' = true,
    scheduled' = scheduled, constructed' = constructed, thread' = thread, stopFlag' = stopFlag,
    lock' = lock, shutdown' = shutdown, shutdownCalls' = shutdownCalls, joins' = joins,
    reports' = reports,
  }

  action step = any {
    construct, wakeToReport, finishReport, wakeToStop, exit, beginStop(false), beginStop(true),
    requestStop, joinReturn, shutdownAgain, dropAfterShutdown,
  }

  val threadOnlyWhenScheduled = not(scheduled) implies (thread == NotStarted and reports == 0)
  val flagOnlyAfterStopBegins = stopFlag implies shutdown != NotCalled
  val leavesOnlyOnTheFlag = (thread == StopRequested or thread == Finished) implies stopFlag
  val noJoinWhileHoldingTheLock = shutdown == Joining implies lock != Main
  val returnedMeansNoThread = shutdown == Returned implies (thread == Finished or thread == NotStarted)
  val joinedAtMostOnce = joins <= 1
  val shutdownCallsAfterTheFirstDoNothing = shutdownCalls > 1 implies shutdown == Returned
  val invariant = threadOnlyWhenScheduled and flagOnlyAfterStopBegins and leavesOnlyOnTheFlag
    and noJoinWhileHoldingTheLock and returnedMeansNoThread and joinedAtMostOnce
    and shutdownCallsAfterTheFirstDoNothing

  run zeroIntervalStartsNoThreadTest =
    initWith(false).then(construct).then(beginStop(false)).then(requestStop).then(joinReturn)
      .expect(thread == NotStarted and reports == 0 and joins == 0 and shutdown == Returned)

  run reportThenShutdownTest =
    initWith(true).then(construct).then(wakeToReport).then(finishReport).then(beginStop(false))
      .then(requestStop).then(wakeToStop).then(exit).then(joinReturn)
      .expect(thread == Finished and reports == 1 and joins == 1 and shutdown == Returned)

  run shutdownDuringALongWaitTest =
    initWith(true).then(construct).then(beginStop(false)).then(requestStop).then(wakeToStop)
      .then(exit).then(joinReturn)
      .expect(thread == Finished and reports == 0 and shutdown == Returned)

  run shutdownWhileReportingWaitsForTheReportTest =
    initWith(true).then(construct).then(wakeToReport).then(beginStop(false)).fail()

  run secondShutdownDoesNothingTest =
    initWith(true).then(construct).then(beginStop(false)).then(requestStop).then(wakeToStop)
      .then(exit).then(joinReturn).then(shutdownAgain)
      .expect(shutdownCalls == 2 and joins == 1 and shutdown == Returned)

  run dropWithoutShutdownStopsTheThreadTest =
    initWith(true).then(construct).then(beginStop(true)).then(requestStop).then(wakeToStop)
      .then(exit).then(joinReturn)
      .expect(dropped and thread == Finished and joins == 1 and shutdownCalls == 0)

  run dropAfterShutdownDoesNothingTest =
    initWith(true).then(construct).then(beginStop(false)).then(requestStop).then(wakeToStop)
      .then(exit).then(joinReturn).then(dropAfterShutdown)
      .expect(dropped and joins == 1)

  // Negative controls: steps the protocol must not allow.
  run noReportAfterTheFlagTest =
    initWith(true).then(construct).then(beginStop(false)).then(requestStop).then(wakeToReport).fail()

  run joinCannotReturnBeforeTheThreadLeavesTest =
    initWith(true).then(construct).then(beginStop(false)).then(requestStop).then(joinReturn).fail()

  run noJoinBeforeTheLockIsReleasedTest =
    initWith(true).then(construct).then(beginStop(false)).then(joinReturn).fail()

  run noShutdownAfterTheDropTest =
    initWith(true).then(construct).then(beginStop(true)).then(requestStop).then(wakeToStop)
      .then(exit).then(joinReturn).then(beginStop(false)).fail()
}
