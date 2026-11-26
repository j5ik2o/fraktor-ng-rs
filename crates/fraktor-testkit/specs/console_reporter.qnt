// The scheduled-report thread of MetricsKit (crates/fraktor-testkit/src/
// metrics_kit/metrics_kit_value.rs; design docs/design/issue-183-metrics-kit.md
// §2 "レポーターと報告の形" and §6).
//
// Building the kit starts the thread only when Console is among the reporters
// and the interval is above zero. The thread waits on a Condvar paired with
// the stop flag; on waking it reads the flag under the stop lock and either
// leaves (the flag is set) or releases the lock and then reports (the flag is
// clear), so the stop side can set the flag while a report is printing.
//
// Every stop (shutdown_metrics from any thread, or the drop) runs the same
// steps: take the stop lock, set the flag, release it, notify, then look at
// the reporter-thread state. The first caller to find the thread running
// marks it joining and joins it; a caller that finds it joining waits until
// it is finished; a caller that finds it finished returns at once. The
// guarantee ends where the call returns, not where the flag is set: a report
// begun before the flag can still print before the call returns, and no
// report runs once any call has returned.
module console_reporter {
  type Thread = NotStarted | Waiting | Reporting | Finished
  // The reporter-thread state of the implementation (ReporterThreadState).
  type Handle = Running | Joining | Joined
  type Call = Idle | Flagging | Flagged | Joiner | WaitingForTheJoin | Returned
  type Lock = Free | HeldBy(str)

  // Two threads calling shutdown_metrics, and the drop.
  pure val Shutdowns = Set("A", "B")
  pure val Drop = "D"
  pure val Callers = Shutdowns.union(Set(Drop))

  var console: bool
  var positiveInterval: bool
  var constructed: bool
  var thread: Thread
  var handle: Handle
  var stopFlag: bool
  var stopLock: Lock
  var calls: str -> Call
  var shutdownCalls: int
  var joins: int
  var dropped: bool
  var reports: int
  // A report begun before the flag printed after the flag was set.
  var printedAfterTheFlag: bool

  action initWith(consoleOn: bool, positive: bool): bool = all {
    console' = consoleOn, positiveInterval' = positive, constructed' = false,
    thread' = NotStarted, handle' = Joined, stopFlag' = false, stopLock' = Free,
    calls' = Callers.mapBy(_ => Idle), shutdownCalls' = 0, joins' = 0, dropped' = false,
    reports' = 0, printedAfterTheFlag' = false,
  }

  action init = {
    nondet consoleOn = Set(true, false).oneOf()
    nondet positive = Set(true, false).oneOf()
    initWith(consoleOn, positive)
  }

  // MetricsKit::with_output: spawns the thread only when Console is enabled
  // and the interval is positive.
  action construct = all {
    not(constructed),
    constructed' = true,
    thread' = if (console and positiveInterval) Waiting else NotStarted,
    handle' = if (console and positiveInterval) Running else Joined,
    console' = console, positiveInterval' = positiveInterval, stopFlag' = stopFlag,
    stopLock' = stopLock, calls' = calls, shutdownCalls' = shutdownCalls, joins' = joins,
    dropped' = dropped, reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
  }

  // wait_timeout_while returns after the interval with the flag clear: the
  // thread releases the stop lock and starts a report.
  action wakeToReport = all {
    thread == Waiting,
    stopLock == Free,
    not(stopFlag),
    reports < 3,
    thread' = Reporting,
    reports' = reports + 1,
    console' = console, positiveInterval' = positiveInterval, constructed' = constructed,
    handle' = handle, stopFlag' = stopFlag, stopLock' = stopLock, calls' = calls,
    shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  // report_once prints without the stop lock; the thread goes back to waiting.
  action finishReport = all {
    thread == Reporting,
    thread' = Waiting,
    printedAfterTheFlag' = (printedAfterTheFlag or stopFlag),
    console' = console, positiveInterval' = positiveInterval, constructed' = constructed,
    handle' = handle, stopFlag' = stopFlag, stopLock' = stopLock, calls' = calls,
    shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped, reports' = reports,
  }

  // The notify (or any wake) with the flag set: the thread leaves at once,
  // however long the interval is.
  action wakeToStop = all {
    thread == Waiting,
    stopLock == Free,
    stopFlag,
    thread' = Finished,
    console' = console, positiveInterval' = positiveInterval, constructed' = constructed,
    handle' = handle, stopFlag' = stopFlag, stopLock' = stopLock, calls' = calls,
    shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped, reports' = reports,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  // A stop begins by taking the stop lock.
  action takeTheStopLock(c: str, byDrop: bool): bool = all {
    stopLock == Free,
    stopLock' = HeldBy(c),
    calls' = calls.set(c, Flagging),
    shutdownCalls' = if (byDrop) shutdownCalls else shutdownCalls + 1,
    dropped' = (dropped or byDrop),
    console' = console, positiveInterval' = positiveInterval, constructed' = constructed,
    thread' = thread, handle' = handle, stopFlag' = stopFlag, joins' = joins,
    reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
  }

  // shutdown_metrics, the first call or a later one, from any thread that
  // still holds the kit.
  action beginShutdown(c: str): bool = all {
    Shutdowns.contains(c),
    constructed,
    not(dropped),
    calls.get(c) == Idle or calls.get(c) == Returned,
    shutdownCalls < 4,
    takeTheStopLock(c, false),
  }

  // The drop owns the kit, so no shutdown_metrics is running and none can
  // start afterwards.
  action beginDrop = all {
    constructed,
    not(dropped),
    Shutdowns.forall(c => calls.get(c) == Idle or calls.get(c) == Returned),
    takeTheStopLock(Drop, true),
  }

  // Set the flag and release the stop lock, then notify.
  action setTheFlag(c: str): bool = all {
    calls.get(c) == Flagging,
    stopLock == HeldBy(c),
    stopFlag' = true,
    stopLock' = Free,
    calls' = calls.set(c, Flagged),
    console' = console, positiveInterval' = positiveInterval, constructed' = constructed,
    thread' = thread, handle' = handle, shutdownCalls' = shutdownCalls, joins' = joins,
    dropped' = dropped, reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
  }

  // Under the reporter-thread lock: join a running thread, wait for a joining
  // one, or return when it is finished (or never started).
  action lookAtTheThread(c: str): bool = all {
    calls.get(c) == Flagged,
    calls' = calls.set(c, match handle {
      | Running => Joiner
      | Joining => WaitingForTheJoin
      | Joined => Returned
    }),
    handle' = if (handle == Running) Joining else handle,
    console' = console, positiveInterval' = positiveInterval, constructed' = constructed,
    thread' = thread, stopFlag' = stopFlag, stopLock' = stopLock,
    shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped, reports' = reports,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  // join returns once the thread has left; the joiner marks it finished and
  // wakes the waiting callers.
  action joinReturn(c: str): bool = all {
    calls.get(c) == Joiner,
    thread == Finished,
    calls' = calls.set(c, Returned),
    handle' = Joined,
    joins' = joins + 1,
    console' = console, positiveInterval' = positiveInterval, constructed' = constructed,
    thread' = thread, stopFlag' = stopFlag, stopLock' = stopLock,
    shutdownCalls' = shutdownCalls, dropped' = dropped, reports' = reports,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  // A caller that found the thread joining returns once it is finished.
  action waitReturn(c: str): bool = all {
    calls.get(c) == WaitingForTheJoin,
    handle == Joined,
    calls' = calls.set(c, Returned),
    console' = console, positiveInterval' = positiveInterval, constructed' = constructed,
    thread' = thread, handle' = handle, stopFlag' = stopFlag, stopLock' = stopLock,
    shutdownCalls' = shutdownCalls, joins' = joins, dropped' = dropped, reports' = reports,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  action step = any {
    construct, wakeToReport, finishReport, wakeToStop, beginDrop,
    { nondet c = Shutdowns.oneOf() beginShutdown(c) },
    {
      nondet c = Callers.oneOf()
      any { setTheFlag(c), lookAtTheThread(c), joinReturn(c), waitReturn(c) }
    },
  }

  val threadOnlyWithConsoleAndInterval =
    not(console and positiveInterval) implies (thread == NotStarted and reports == 0)
  val flagOnlyAfterAStopBegins = stopFlag implies Callers.exists(c => calls.get(c) != Idle)
  val leavesOnlyOnTheFlag = thread == Finished implies stopFlag
  val stopLockOnlyAroundTheFlag =
    Callers.forall(c => stopLock == HeldBy(c) implies calls.get(c) == Flagging)
  // The boundary of the guarantee: once any call has returned, no report is
  // printing and none can begin.
  val noReportOnceACallReturned =
    Callers.forall(c => calls.get(c) == Returned implies (thread == Finished or thread == NotStarted))
  val oneJoinerAtATime = Callers.filter(c => calls.get(c) == Joiner).size() <= 1
  val joinedAtMostOnce = joins <= 1
  val nothingRunsAfterTheDrop =
    dropped implies Shutdowns.forall(c => calls.get(c) == Idle or calls.get(c) == Returned)
  val invariant = threadOnlyWithConsoleAndInterval and flagOnlyAfterAStopBegins
    and leavesOnlyOnTheFlag and stopLockOnlyAroundTheFlag and noReportOnceACallReturned
    and oneJoinerAtATime and joinedAtMostOnce and nothingRunsAfterTheDrop

  run zeroIntervalStartsNoThreadTest =
    initWith(true, false).then(construct).then(beginShutdown("A")).then(setTheFlag("A")).then(lookAtTheThread("A"))
      .expect(thread == NotStarted and reports == 0 and joins == 0 and calls.get("A") == Returned)

  run noConsoleStartsNoThreadTest =
    initWith(false, true).then(construct)
      .expect(thread == NotStarted and handle == Joined)
      .then(beginShutdown("A")).then(setTheFlag("A")).then(lookAtTheThread("A"))
      .expect(thread == NotStarted and reports == 0 and joins == 0 and calls.get("A") == Returned)

  run noConsoleNeverReportsTest =
    initWith(false, true).then(construct).then(wakeToReport).fail()

  run reportThenShutdownTest =
    initWith(true, true).then(construct).then(wakeToReport).then(finishReport)
      .then(beginShutdown("A")).then(setTheFlag("A")).then(lookAtTheThread("A"))
      .then(wakeToStop).then(joinReturn("A"))
      .expect(thread == Finished and reports == 1 and joins == 1 and calls.get("A") == Returned
        and not(printedAfterTheFlag))

  run shutdownDuringALongWaitTest =
    initWith(true, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(wakeToStop).then(lookAtTheThread("A")).then(joinReturn("A"))
      .expect(thread == Finished and reports == 0 and calls.get("A") == Returned)

  // The stop lock is free while the report prints: the stop side sets the
  // flag and goes on to join, and the report still prints before join returns.
  run theFlagIsSetWhileAReportPrintsTest =
    initWith(true, true).then(construct).then(wakeToReport).then(beginShutdown("A"))
      .then(setTheFlag("A")).then(lookAtTheThread("A"))
      .expect(stopFlag and thread == Reporting and calls.get("A") == Joiner)
      .then(finishReport).then(wakeToStop).then(joinReturn("A"))
      .expect(printedAfterTheFlag and reports == 1 and calls.get("A") == Returned)

  run concurrentShutdownWaitsForTheJoinTest =
    initWith(true, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(lookAtTheThread("A")).then(beginShutdown("B")).then(setTheFlag("B"))
      .then(lookAtTheThread("B"))
      .expect(calls.get("A") == Joiner and calls.get("B") == WaitingForTheJoin)
      .then(wakeToStop).then(joinReturn("A")).then(waitReturn("B"))
      .expect(joins == 1 and calls.get("B") == Returned)

  run secondShutdownDoesNothingTest =
    initWith(true, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(wakeToStop).then(lookAtTheThread("A")).then(joinReturn("A"))
      .then(beginShutdown("A")).then(setTheFlag("A")).then(lookAtTheThread("A"))
      .expect(shutdownCalls == 2 and joins == 1 and calls.get("A") == Returned)

  run dropWithoutShutdownStopsTheThreadTest =
    initWith(true, true).then(construct).then(beginDrop).then(setTheFlag(Drop))
      .then(wakeToStop).then(lookAtTheThread(Drop)).then(joinReturn(Drop))
      .expect(dropped and thread == Finished and joins == 1 and shutdownCalls == 0)

  run dropAfterShutdownDoesNothingTest =
    initWith(true, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(wakeToStop).then(lookAtTheThread("A")).then(joinReturn("A"))
      .then(beginDrop).then(setTheFlag(Drop)).then(lookAtTheThread(Drop))
      .expect(dropped and joins == 1 and calls.get(Drop) == Returned)

  // Negative controls: steps the protocol must not allow.

  // The boundary is the return: after it no report begins.
  run noReportAfterTheReturnTest =
    initWith(true, true).then(construct).then(wakeToReport).then(beginShutdown("A"))
      .then(setTheFlag("A")).then(lookAtTheThread("A")).then(finishReport).then(wakeToStop)
      .then(joinReturn("A")).then(wakeToReport).fail()

  run joinCannotReturnWhileAReportPrintsTest =
    initWith(true, true).then(construct).then(wakeToReport).then(beginShutdown("A"))
      .then(setTheFlag("A")).then(lookAtTheThread("A")).then(joinReturn("A")).fail()

  run joinCannotReturnBeforeTheThreadLeavesTest =
    initWith(true, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(lookAtTheThread("A")).then(joinReturn("A")).fail()

  run aWaitingCallerCannotReturnBeforeTheJoinTest =
    initWith(true, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(lookAtTheThread("A")).then(beginShutdown("B")).then(setTheFlag("B"))
      .then(lookAtTheThread("B")).then(wakeToStop).then(waitReturn("B")).fail()

  run theThreadCannotWakeWhileTheStopLockIsHeldTest =
    initWith(true, true).then(construct).then(beginShutdown("A")).then(wakeToStop).fail()

  // The same shutdown_metrics that succeeds before the drop (see
  // secondShutdownDoesNothingTest) is impossible after it.
  run noShutdownAfterTheDropTest =
    initWith(true, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(wakeToStop).then(lookAtTheThread("A")).then(joinReturn("A"))
      .then(beginDrop).then(setTheFlag(Drop)).then(lookAtTheThread(Drop))
      .then(beginShutdown("A")).fail()

  run noShutdownAfterADropWithoutShutdownTest =
    initWith(true, true).then(construct).then(beginDrop).then(setTheFlag(Drop))
      .then(wakeToStop).then(lookAtTheThread(Drop)).then(joinReturn(Drop))
      .then(beginShutdown("A")).fail()
}
