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
//
// A report (the thread's, or a caller's report_metrics and
// report_and_clear_metrics) takes two locks in this order: the output Mutex
// of the kit (metrics_kit_value.rs line 68), then, inside each call into the
// writer (every write and the flush at the end), the lock of the writer
// itself. The writer of MetricsKit::new is the standard
// output, whose lock is the re-entrant StdoutLock that any thread can also
// take; a writer given to with_output may take a lock that callers can hold
// (a shared buffer behind a Mutex, which is not re-entrant) or none. The
// contract of the design (docs/design/issue-183-metrics-kit.md §2 "出力のロックの
// 契約") is that a thread does not stop the kit, drop it, or report through it
// while it holds the writer's lock. The contract also keeps a call into the
// writer from stopping, dropping or reporting through the same kit; this
// model does not have that part (the design, §6 (d)).
// `contract` turns the contract on (the default `init`) or off (the runs that
// show the deadlocks it prevents); `deadlocked` is a cycle of waits, defined
// apart from the guards.
module console_reporter {
  type Thread = NotStarted | Waiting | Reporting | Finished
  // The reporter-thread state of the implementation (ReporterThreadState).
  type Handle = Running | Joining | Joined
  type Call = Idle | Flagging | Flagged | Joiner | WaitingForTheJoin | Returned
  // Where a report is: not printing, waiting for the output Mutex, or
  // holding it while it writes.
  type Print = NoPrint | WantsTheOutput | HasTheOutput
  type Lock = Free | HeldBy(str)
  // The lock the writer takes in each call into it (write and flush).
  type Writer = NoSharedLock | ReentrantLock | PlainLock

  // Two threads that call shutdown_metrics and report_metrics, and the
  // thread that drops the kit.
  pure val Shutdowns = Set("A", "B")
  pure val Drop = "D"
  pure val Callers = Shutdowns.union(Set(Drop))
  // The scheduled report thread.
  pure val ReporterThread = "T"
  pure val Actors = Callers.union(Set(ReporterThread))
  pure val Writers = Set(NoSharedLock, ReentrantLock, PlainLock)

  var console: bool
  var positiveInterval: bool
  var writer: Writer
  var contract: bool
  var constructed: bool
  var thread: Thread
  var handle: Handle
  var stopFlag: bool
  var stopLock: Lock
  // output: Mutex<Box<dyn Write + Send>> of the kit.
  var outputMutex: Lock
  // The writer's own lock (the StdoutLock, or the lock of a shared buffer).
  var writerLock: Lock
  // How many times a caller took the writer's lock; bounded so that a stuck
  // state ends the trace instead of being hidden by take-and-release loops.
  var writerHolds: int
  var printing: str -> Print
  var calls: str -> Call
  var shutdownCalls: int
  var callerReports: int
  var joins: int
  var dropped: bool
  var reports: int
  // A report begun before the flag printed after the flag was set.
  var printedAfterTheFlag: bool

  pure def holderOf(l: Lock): Set[str] = match l {
    | Free => Set()
    | HeldBy(h) => Set(h)
  }

  // Not inside a stop or a report, so the thread can take or release locks.
  def isFree(c: str): bool =
    (calls.get(c) == Idle or calls.get(c) == Returned) and printing.get(c) == NoPrint

  // A write by x goes through unless another thread holds the writer's lock
  // (or x itself holds a lock that is not re-entrant).
  def canWrite(x: str): bool =
    writer == NoSharedLock or writerLock == Free or (writer == ReentrantLock and writerLock == HeldBy(x))

  // The threads x waits for; empty when x can take its next step.
  def waitsFor(x: str): Set[str] =
    val forTheOutput =
      if (printing.get(x) == WantsTheOutput) holderOf(outputMutex).exclude(Set(x)) else Set()
    val forTheWriter =
      if (printing.get(x) == HasTheOutput and not(canWrite(x))) holderOf(writerLock) else Set()
    val forTheThread =
      if (x != ReporterThread and calls.get(x) == Joiner and thread != Finished) Set(ReporterThread) else Set()
    val forTheJoiner =
      if (x != ReporterThread and calls.get(x) == WaitingForTheJoin and handle != Joined)
        Callers.filter(j => calls.get(j) == Joiner)
      else Set()
    forTheOutput.union(forTheWriter).union(forTheThread).union(forTheJoiner)

  def waitsThrough(s: Set[str]): Set[str] = s.union(s.map(y => waitsFor(y)).flatten())

  // A cycle of waits among the four threads: every thread in it waits for
  // another in it, so none of them can move again.
  val deadlocked = Actors.exists(x => waitsThrough(waitsThrough(waitsThrough(waitsFor(x)))).contains(x))

  action initWith(consoleOn: bool, positive: bool, w: Writer, honoured: bool): bool = all {
    console' = consoleOn, positiveInterval' = positive, writer' = w, contract' = honoured,
    constructed' = false,
    thread' = NotStarted, handle' = Joined, stopFlag' = false, stopLock' = Free,
    outputMutex' = Free, writerLock' = Free, writerHolds' = 0,
    printing' = Actors.mapBy(_ => NoPrint),
    calls' = Callers.mapBy(_ => Idle), shutdownCalls' = 0, callerReports' = 0, joins' = 0,
    dropped' = false, reports' = 0, printedAfterTheFlag' = false,
  }

  action init = {
    nondet consoleOn = Set(true, false).oneOf()
    nondet positive = Set(true, false).oneOf()
    nondet w = Writers.oneOf()
    initWith(consoleOn, positive, w, true)
  }

  // MetricsKit::with_output: spawns the thread only when Console is enabled
  // and the interval is positive.
  action construct = all {
    not(constructed),
    constructed' = true,
    thread' = if (console and positiveInterval) Waiting else NotStarted,
    handle' = if (console and positiveInterval) Running else Joined,
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    stopFlag' = stopFlag, stopLock' = stopLock, outputMutex' = outputMutex, writerLock' = writerLock,
    writerHolds' = writerHolds, printing' = printing, calls' = calls, shutdownCalls' = shutdownCalls,
    callerReports' = callerReports, joins' = joins, dropped' = dropped, reports' = reports,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  // wait_timeout_while returns after the interval with the flag clear: the
  // thread releases the stop lock and starts a report.
  action wakeToReport = all {
    thread == Waiting,
    stopLock == Free,
    not(stopFlag),
    reports < 3,
    thread' = Reporting,
    printing' = printing.set(ReporterThread, WantsTheOutput),
    reports' = reports + 1,
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, handle' = handle, stopFlag' = stopFlag, stopLock' = stopLock,
    outputMutex' = outputMutex, writerLock' = writerLock, writerHolds' = writerHolds, calls' = calls,
    shutdownCalls' = shutdownCalls, callerReports' = callerReports, joins' = joins, dropped' = dropped,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  // report_metrics or report_and_clear_metrics on a caller's thread. Without
  // a console reporter it prints nothing and takes no lock, so it is not a
  // step here.
  action beginReport(c: str): bool = all {
    Shutdowns.contains(c),
    constructed,
    not(dropped),
    console,
    isFree(c),
    callerReports < 2,
    // The contract: release the writer's lock before reporting.
    contract implies writerLock != HeldBy(c),
    printing' = printing.set(c, WantsTheOutput),
    callerReports' = callerReports + 1,
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, thread' = thread, handle' = handle, stopFlag' = stopFlag,
    stopLock' = stopLock, outputMutex' = outputMutex, writerLock' = writerLock,
    writerHolds' = writerHolds, calls' = calls, shutdownCalls' = shutdownCalls, joins' = joins,
    dropped' = dropped, reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
  }

  // report_snapshot takes the output Mutex (line 68).
  action takeTheOutput(x: str): bool = all {
    printing.get(x) == WantsTheOutput,
    outputMutex == Free,
    outputMutex' = HeldBy(x),
    printing' = printing.set(x, HasTheOutput),
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, thread' = thread, handle' = handle, stopFlag' = stopFlag,
    stopLock' = stopLock, writerLock' = writerLock, writerHolds' = writerHolds, calls' = calls,
    shutdownCalls' = shutdownCalls, callerReports' = callerReports, joins' = joins, dropped' = dropped,
    reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
  }

  // The writes go through the writer's lock, then report_snapshot releases
  // the output Mutex; the thread goes back to waiting.
  action finishPrinting(x: str): bool = all {
    printing.get(x) == HasTheOutput,
    outputMutex == HeldBy(x),
    canWrite(x),
    outputMutex' = Free,
    printing' = printing.set(x, NoPrint),
    thread' = if (x == ReporterThread) Waiting else thread,
    printedAfterTheFlag' = (printedAfterTheFlag or (x == ReporterThread and stopFlag)),
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, handle' = handle, stopFlag' = stopFlag, stopLock' = stopLock,
    writerLock' = writerLock, writerHolds' = writerHolds, calls' = calls,
    shutdownCalls' = shutdownCalls, callerReports' = callerReports, joins' = joins, dropped' = dropped,
    reports' = reports,
  }

  // A caller takes the writer's lock itself (std::io::stdout().lock(), or the
  // Mutex of a shared buffer), outside any stop or report.
  action holdTheWriter(c: str): bool = all {
    writer != NoSharedLock,
    writerLock == Free,
    isFree(c),
    writerHolds < 2,
    writerLock' = HeldBy(c), writerHolds' = writerHolds + 1,
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, thread' = thread, handle' = handle, stopFlag' = stopFlag,
    stopLock' = stopLock, outputMutex' = outputMutex, printing' = printing, calls' = calls,
    shutdownCalls' = shutdownCalls, callerReports' = callerReports, joins' = joins, dropped' = dropped,
    reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
  }

  // A thread blocked in a stop or a report cannot release what it holds.
  action releaseTheWriter(c: str): bool = all {
    writerLock == HeldBy(c),
    isFree(c),
    writerLock' = Free, writerHolds' = writerHolds,
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, thread' = thread, handle' = handle, stopFlag' = stopFlag,
    stopLock' = stopLock, outputMutex' = outputMutex, printing' = printing, calls' = calls,
    shutdownCalls' = shutdownCalls, callerReports' = callerReports, joins' = joins, dropped' = dropped,
    reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
  }

  // The notify (or any wake) with the flag set: the thread leaves at once,
  // however long the interval is.
  action wakeToStop = all {
    thread == Waiting,
    stopLock == Free,
    stopFlag,
    thread' = Finished,
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, handle' = handle, stopFlag' = stopFlag, stopLock' = stopLock,
    outputMutex' = outputMutex, writerLock' = writerLock, writerHolds' = writerHolds,
    printing' = printing, calls' = calls, shutdownCalls' = shutdownCalls,
    callerReports' = callerReports, joins' = joins, dropped' = dropped, reports' = reports,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  // A stop begins by taking the stop lock.
  action takeTheStopLock(c: str, byDrop: bool): bool = all {
    stopLock == Free,
    stopLock' = HeldBy(c),
    calls' = calls.set(c, Flagging),
    shutdownCalls' = if (byDrop) shutdownCalls else shutdownCalls + 1,
    dropped' = (dropped or byDrop),
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, thread' = thread, handle' = handle, stopFlag' = stopFlag,
    outputMutex' = outputMutex, writerLock' = writerLock, writerHolds' = writerHolds,
    printing' = printing, callerReports' = callerReports, joins' = joins, reports' = reports,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  // shutdown_metrics, the first call or a later one, from any thread that
  // still holds the kit.
  action beginShutdown(c: str): bool = all {
    Shutdowns.contains(c),
    constructed,
    not(dropped),
    isFree(c),
    shutdownCalls < 4,
    // The contract: release the writer's lock before stopping.
    contract implies writerLock != HeldBy(c),
    takeTheStopLock(c, false),
  }

  // The drop owns the kit, so no shutdown_metrics or report is running and
  // none can start afterwards.
  action beginDrop = all {
    constructed,
    not(dropped),
    Shutdowns.forall(c => isFree(c)),
    // The contract: release the writer's lock before dropping.
    contract implies writerLock != HeldBy(Drop),
    takeTheStopLock(Drop, true),
  }

  // Set the flag and release the stop lock, then notify.
  action setTheFlag(c: str): bool = all {
    calls.get(c) == Flagging,
    stopLock == HeldBy(c),
    stopFlag' = true,
    stopLock' = Free,
    calls' = calls.set(c, Flagged),
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, thread' = thread, handle' = handle, outputMutex' = outputMutex,
    writerLock' = writerLock, writerHolds' = writerHolds, printing' = printing,
    shutdownCalls' = shutdownCalls, callerReports' = callerReports, joins' = joins, dropped' = dropped,
    reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
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
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, thread' = thread, stopFlag' = stopFlag, stopLock' = stopLock,
    outputMutex' = outputMutex, writerLock' = writerLock, writerHolds' = writerHolds,
    printing' = printing, shutdownCalls' = shutdownCalls, callerReports' = callerReports,
    joins' = joins, dropped' = dropped, reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
  }

  // join returns once the thread has left; the joiner marks it finished and
  // wakes the waiting callers.
  action joinReturn(c: str): bool = all {
    calls.get(c) == Joiner,
    thread == Finished,
    calls' = calls.set(c, Returned),
    handle' = Joined,
    joins' = joins + 1,
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, thread' = thread, stopFlag' = stopFlag, stopLock' = stopLock,
    outputMutex' = outputMutex, writerLock' = writerLock, writerHolds' = writerHolds,
    printing' = printing, shutdownCalls' = shutdownCalls, callerReports' = callerReports,
    dropped' = dropped, reports' = reports, printedAfterTheFlag' = printedAfterTheFlag,
  }

  // A caller that found the thread joining returns once it is finished.
  action waitReturn(c: str): bool = all {
    calls.get(c) == WaitingForTheJoin,
    handle == Joined,
    calls' = calls.set(c, Returned),
    console' = console, positiveInterval' = positiveInterval, writer' = writer, contract' = contract,
    constructed' = constructed, thread' = thread, handle' = handle, stopFlag' = stopFlag,
    stopLock' = stopLock, outputMutex' = outputMutex, writerLock' = writerLock,
    writerHolds' = writerHolds, printing' = printing, shutdownCalls' = shutdownCalls,
    callerReports' = callerReports, joins' = joins, dropped' = dropped, reports' = reports,
    printedAfterTheFlag' = printedAfterTheFlag,
  }

  action step = any {
    construct, wakeToReport, wakeToStop, beginDrop,
    { nondet x = Actors.oneOf() any { takeTheOutput(x), finishPrinting(x) } },
    { nondet c = Shutdowns.oneOf() any { beginShutdown(c), beginReport(c) } },
    {
      nondet c = Callers.oneOf()
      any {
        setTheFlag(c), lookAtTheThread(c), joinReturn(c), waitReturn(c),
        holdTheWriter(c), releaseTheWriter(c),
      }
    },
  }

  val threadOnlyWithConsoleAndInterval =
    not(console and positiveInterval) implies (thread == NotStarted and reports == 0)
  val flagOnlyAfterAStopBegins = stopFlag implies Callers.exists(c => calls.get(c) != Idle)
  val leavesOnlyOnTheFlag = thread == Finished implies stopFlag
  val stopLockOnlyAroundTheFlag =
    Callers.forall(c => stopLock == HeldBy(c) implies calls.get(c) == Flagging)
  // The boundary of the guarantee: once any call has returned, no report of
  // the thread is printing and none can begin.
  val noReportOnceACallReturned =
    Callers.forall(c => calls.get(c) == Returned implies (thread == Finished or thread == NotStarted))
  val oneJoinerAtATime = Callers.filter(c => calls.get(c) == Joiner).size() <= 1
  val joinedAtMostOnce = joins <= 1
  val nothingRunsAfterTheDrop = dropped implies Shutdowns.forall(c => isFree(c))
  // The contract, as a state: a thread inside a stop or a report never holds
  // the writer's lock.
  val noStopOrReportHoldsTheWriter =
    Callers.forall(c => writerLock == HeldBy(c) implies isFree(c))
  // Only a report that is printing holds the output Mutex.
  val theOutputIsHeldOnlyToPrint =
    Actors.forall(x => outputMutex == HeldBy(x) implies printing.get(x) == HasTheOutput)
      and (printing.get(ReporterThread) != NoPrint implies thread == Reporting)
  // What the contract is for: no cycle of waits.
  val noDeadlock = not(deadlocked)
  val invariant = threadOnlyWithConsoleAndInterval and flagOnlyAfterAStopBegins
    and leavesOnlyOnTheFlag and stopLockOnlyAroundTheFlag and noReportOnceACallReturned
    and oneJoinerAtATime and joinedAtMostOnce and nothingRunsAfterTheDrop
    and noStopOrReportHoldsTheWriter and theOutputIsHeldOnlyToPrint and noDeadlock

  // The thread's report, from the wake to the wait.
  action reportOnce = wakeToReport.then(takeTheOutput(ReporterThread)).then(finishPrinting(ReporterThread))

  run zeroIntervalStartsNoThreadTest =
    initWith(true, false, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(lookAtTheThread("A"))
      .expect(thread == NotStarted and reports == 0 and joins == 0 and calls.get("A") == Returned)

  run noConsoleStartsNoThreadTest =
    initWith(false, true, ReentrantLock, true).then(construct)
      .expect(thread == NotStarted and handle == Joined)
      .then(beginShutdown("A")).then(setTheFlag("A")).then(lookAtTheThread("A"))
      .expect(thread == NotStarted and reports == 0 and joins == 0 and calls.get("A") == Returned)

  run noConsoleNeverReportsTest =
    initWith(false, true, ReentrantLock, true).then(construct).then(wakeToReport).fail()

  run noConsoleCallerReportTakesNoLockTest =
    initWith(false, true, ReentrantLock, true).then(construct).then(beginReport("A")).fail()

  run reportThenShutdownTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(reportOnce)
      .then(beginShutdown("A")).then(setTheFlag("A")).then(lookAtTheThread("A"))
      .then(wakeToStop).then(joinReturn("A"))
      .expect(thread == Finished and reports == 1 and joins == 1 and calls.get("A") == Returned
        and not(printedAfterTheFlag))

  run shutdownDuringALongWaitTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(wakeToStop).then(lookAtTheThread("A")).then(joinReturn("A"))
      .expect(thread == Finished and reports == 0 and calls.get("A") == Returned)

  // The stop lock is free while the report prints: the stop side sets the
  // flag and goes on to join, and the report still prints before join returns.
  run theFlagIsSetWhileAReportPrintsTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(wakeToReport).then(beginShutdown("A"))
      .then(setTheFlag("A")).then(lookAtTheThread("A"))
      .expect(stopFlag and thread == Reporting and calls.get("A") == Joiner and waitsFor("A") == Set("T"))
      .then(takeTheOutput("T")).then(finishPrinting("T")).then(wakeToStop).then(joinReturn("A"))
      .expect(printedAfterTheFlag and reports == 1 and calls.get("A") == Returned)

  run concurrentShutdownWaitsForTheJoinTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(lookAtTheThread("A")).then(beginShutdown("B")).then(setTheFlag("B"))
      .then(lookAtTheThread("B"))
      .expect(calls.get("A") == Joiner and calls.get("B") == WaitingForTheJoin and waitsFor("B") == Set("A"))
      .then(wakeToStop).then(joinReturn("A")).then(waitReturn("B"))
      .expect(joins == 1 and calls.get("B") == Returned)

  run secondShutdownDoesNothingTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(wakeToStop).then(lookAtTheThread("A")).then(joinReturn("A"))
      .then(beginShutdown("A")).then(setTheFlag("A")).then(lookAtTheThread("A"))
      .expect(shutdownCalls == 2 and joins == 1 and calls.get("A") == Returned)

  run dropWithoutShutdownStopsTheThreadTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginDrop).then(setTheFlag(Drop))
      .then(wakeToStop).then(lookAtTheThread(Drop)).then(joinReturn(Drop))
      .expect(dropped and thread == Finished and joins == 1 and shutdownCalls == 0)

  run dropAfterShutdownDoesNothingTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(wakeToStop).then(lookAtTheThread("A")).then(joinReturn("A"))
      .then(beginDrop).then(setTheFlag(Drop)).then(lookAtTheThread(Drop))
      .expect(dropped and joins == 1 and calls.get(Drop) == Returned)

  // A caller's report waits for the thread's report, then prints.
  run aCallerReportWaitsForTheThreadsReportTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(wakeToReport).then(takeTheOutput("T"))
      .then(beginReport("A"))
      .expect(waitsFor("A") == Set("T") and not(deadlocked))
      .then(finishPrinting("T")).then(takeTheOutput("A")).then(finishPrinting("A"))
      .expect(printing.get("A") == NoPrint and outputMutex == Free and callerReports == 1)

  // The reviewer's trace, with the holder of the writer's lock (A) another
  // thread than the one that drops (D): the drop waits for the thread, the
  // thread waits for A, and A is free to release, so the drop is only
  // delayed.
  run aHolderOtherThanTheDropperOnlyDelaysTheDropTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(wakeToReport).then(holdTheWriter("A"))
      .then(beginDrop).then(setTheFlag(Drop)).then(lookAtTheThread(Drop)).then(takeTheOutput("T"))
      .expect(calls.get(Drop) == Joiner and waitsFor(Drop) == Set("T") and waitsFor("T") == Set("A")
        and waitsFor("A") == Set() and not(deadlocked))
      .then(releaseTheWriter("A")).then(finishPrinting("T")).then(wakeToStop).then(joinReturn(Drop))
      .expect(calls.get(Drop) == Returned and joins == 1 and writerLock == Free)

  // The same for a shutdown_metrics on another thread.
  run aHolderOtherThanTheStopperOnlyDelaysTheStopTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(wakeToReport).then(holdTheWriter("B"))
      .then(beginShutdown("A")).then(setTheFlag("A")).then(lookAtTheThread("A")).then(takeTheOutput("T"))
      .expect(calls.get("A") == Joiner and waitsFor("T") == Set("B") and not(deadlocked))
      .then(releaseTheWriter("B")).then(finishPrinting("T")).then(wakeToStop).then(joinReturn("A"))
      .expect(calls.get("A") == Returned and joins == 1 and writerLock == Free)

  // A caller that released the writer's lock first stops as usual.
  run releasingTheWriterBeforeTheStopTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(holdTheWriter("A")).then(releaseTheWriter("A"))
      .then(beginShutdown("A")).then(setTheFlag("A")).then(wakeToStop).then(lookAtTheThread("A"))
      .then(joinReturn("A"))
      .expect(calls.get("A") == Returned and joins == 1)

  // Without a reporter thread, the re-entrant StdoutLock lets the holder
  // report through it; the contract still asks to release it first.
  run aReentrantWriterLetsItsHolderPrintTest =
    initWith(true, false, ReentrantLock, false).then(construct).then(holdTheWriter("A")).then(beginReport("A"))
      .then(takeTheOutput("A")).then(finishPrinting("A"))
      .expect(printing.get("A") == NoPrint and not(deadlocked))

  // A writer that takes no lock others can hold never makes a report wait
  // for a caller.
  run noSharedLockCannotBeHeldTest =
    initWith(true, true, NoSharedLock, true).then(construct).then(holdTheWriter("A")).fail()

  // What the contract prevents, with the contract off. Each run ends in a
  // cycle of waits.

  // The thread that drops the kit holds the writer's lock: the thread waits
  // for it, and the drop waits for the thread.
  run theDropperHoldingTheWriterDeadlocksTest =
    initWith(true, true, ReentrantLock, false).then(construct).then(wakeToReport).then(holdTheWriter(Drop))
      .then(takeTheOutput("T")).then(beginDrop).then(setTheFlag(Drop)).then(lookAtTheThread(Drop))
      .expect(deadlocked and waitsFor(Drop) == Set("T") and waitsFor("T") == Set(Drop))

  run theStopperHoldingTheWriterDeadlocksTest =
    initWith(true, true, PlainLock, false).then(construct).then(wakeToReport).then(holdTheWriter("A"))
      .then(takeTheOutput("T")).then(beginShutdown("A")).then(setTheFlag("A")).then(lookAtTheThread("A"))
      .expect(deadlocked and not(noStopOrReportHoldsTheWriter))

  // No stop at all: a caller holding the StdoutLock calls report_metrics
  // while the thread holds the output Mutex and waits for the StdoutLock.
  run aReportHoldingTheWriterDeadlocksWithTheThreadTest =
    initWith(true, true, ReentrantLock, false).then(construct).then(wakeToReport).then(holdTheWriter("A"))
      .then(takeTheOutput("T")).then(beginReport("A"))
      .expect(deadlocked and waitsFor("A") == Set("T") and waitsFor("T") == Set("A"))

  // A lock that is not re-entrant (a shared buffer's Mutex) deadlocks its
  // holder's report on itself, with no thread.
  run aReportHoldingAPlainLockDeadlocksOnItselfTest =
    initWith(true, false, PlainLock, false).then(construct).then(holdTheWriter("A")).then(beginReport("A"))
      .then(takeTheOutput("A"))
      .expect(deadlocked and waitsFor("A") == Set("A"))

  // Negative controls: steps the protocol must not allow.

  // The contract: a thread holding the writer's lock does not stop, drop or
  // report.
  run noShutdownWhileHoldingTheWriterTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(holdTheWriter("A")).then(beginShutdown("A")).fail()

  run noDropWhileHoldingTheWriterTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(holdTheWriter(Drop)).then(beginDrop).fail()

  run noReportWhileHoldingTheWriterTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(holdTheWriter("A")).then(beginReport("A")).fail()

  // A thread blocked in a join cannot release the writer's lock.
  run aJoinerCannotReleaseTest =
    initWith(true, true, ReentrantLock, false).then(construct).then(holdTheWriter("A")).then(beginShutdown("A"))
      .then(setTheFlag("A")).then(lookAtTheThread("A")).then(releaseTheWriter("A")).fail()

  // The report waits for the writer's lock.
  run theReportWaitsForTheWriterTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(holdTheWriter("A")).then(wakeToReport)
      .then(takeTheOutput("T")).then(finishPrinting("T")).fail()

  // The boundary is the return: after it no report begins.
  run noReportAfterTheReturnTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(wakeToReport).then(beginShutdown("A"))
      .then(setTheFlag("A")).then(lookAtTheThread("A")).then(takeTheOutput("T")).then(finishPrinting("T"))
      .then(wakeToStop)
      .then(joinReturn("A")).then(wakeToReport).fail()

  run joinCannotReturnWhileAReportPrintsTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(wakeToReport).then(beginShutdown("A"))
      .then(setTheFlag("A")).then(lookAtTheThread("A")).then(joinReturn("A")).fail()

  run joinCannotReturnBeforeTheThreadLeavesTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(lookAtTheThread("A")).then(joinReturn("A")).fail()

  run aWaitingCallerCannotReturnBeforeTheJoinTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(lookAtTheThread("A")).then(beginShutdown("B")).then(setTheFlag("B"))
      .then(lookAtTheThread("B")).then(wakeToStop).then(waitReturn("B")).fail()

  // The flag is already set, so only the stop lock (held by B) keeps the
  // thread from leaving; the twin below shows the same step goes through once
  // B releases it.
  run theThreadCannotWakeToStopWhileTheStopLockIsHeldTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(beginShutdown("B")).then(wakeToStop).fail()

  run theThreadWakesToStopOnceTheStopLockIsFreeTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(beginShutdown("B")).then(setTheFlag("B")).then(wakeToStop)
      .expect(thread == Finished and stopLock == Free)

  // The flag is still clear, so only the stop lock (held by A) keeps the
  // thread from reporting; the twin below shows the same step goes through
  // when no stop holds the lock (A cannot release it without setting the
  // flag).
  run theThreadCannotWakeToReportWhileTheStopLockIsHeldTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(wakeToReport).fail()

  run theThreadWakesToReportOnceTheStopLockIsFreeTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(wakeToReport)
      .expect(thread == Reporting and stopLock == Free and not(stopFlag))

  // The same shutdown_metrics that succeeds before the drop (see
  // secondShutdownDoesNothingTest) is impossible after it.
  run noShutdownAfterTheDropTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginShutdown("A")).then(setTheFlag("A"))
      .then(wakeToStop).then(lookAtTheThread("A")).then(joinReturn("A"))
      .then(beginDrop).then(setTheFlag(Drop)).then(lookAtTheThread(Drop))
      .then(beginShutdown("A")).fail()

  run noShutdownAfterADropWithoutShutdownTest =
    initWith(true, true, ReentrantLock, true).then(construct).then(beginDrop).then(setTheFlag(Drop))
      .then(wakeToStop).then(lookAtTheThread(Drop)).then(joinReturn(Drop))
      .then(beginShutdown("A")).fail()
}
