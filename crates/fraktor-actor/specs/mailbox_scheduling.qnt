// The mailbox of one actor and the dispatcher that schedules it on a pool of
// worker threads (docs/design/issue-234-multithread-executor.md, section 5).
//
// It mirrors Pekko's Mailbox status (dispatch/Mailbox.scala: a closed bit, a
// scheduled bit and a suspend count, changed only by compare-and-set) and the
// run loop of Mailbox.run with Dispatcher.registerForExecution. A sender
// enqueues and then asks the dispatcher to schedule the mailbox. Only the
// sender that wins the scheduled bit submits the mailbox to the pool, and the
// worker that takes the submission is the only one that runs the actor. A run
// handles at most THROUGHPUT messages, applies the system messages itself
// (suspend, resume, terminate), clears the scheduled bit, and then asks to be
// scheduled again in case a message arrived while the bit was still set.
//
// The modes other than Fixed are the mistakes the design avoids. They are
// reached only through initWith in the runs below, so `init` (and the CI's
// `quint run --invariant=invariant`) explores the fixed mailbox only.
//   LostWakeup      : the run clears the scheduled bit and does not ask again.
//   CheckThenAct    : scheduling reads the bit and sets it in two steps.
//   ExternalSuspend : a suspend from outside changes the count directly,
//                     while the runner may be inside `receive`.
//   LateEnqueue     : a sender checks `closed` and enqueues in two steps.
module mailbox_scheduling {
  pure val SENDERS = Set("s1", "s2")
  pure val WORKERS = Set("w1", "w2")
  pure val SENDS_PER_SENDER = 2
  pure val THROUGHPUT = 2
  pure val CONTROL_BUDGET = 2

  type Mode = Fixed | LostWakeup | CheckThenAct | ExternalSuspend | LateEnqueue
  type SysMsg = Suspend | Resume | Terminate
  // SChecked: under CheckThenAct, the sender saw the bit clear and will set it.
  // SEnqueuing: under LateEnqueue, the sender saw the mailbox open and will enqueue.
  type SenderPhase = SReady | SRegistering | SChecked | SEnqueuing
  // Running, Invoking and Finishing hold the scheduled bit. Rescheduling has
  // cleared it and is about to ask the dispatcher again.
  type WorkerPhase = Free | Running | Invoking | Finishing | Rescheduling

  type State = {
    closed: bool,
    scheduled: bool,
    suspendCount: int,
    queue: int,
    sys: List[SysMsg],
    submitted: int,
    controlLeft: int,
    pendingDecision: int,
    sent: int,
    processed: int,
    deadLettered: int,
  }
  type Worker = { phase: WorkerPhase, batch: int }
  type Sender = { phase: SenderPhase, left: int }

  var mode: Mode
  var st: State
  var workers: str -> Worker
  var senders: str -> Sender

  // Mailbox.canBeScheduledForExecution: an open mailbox runs for any message,
  // a suspended one only for a system message, a closed one never.
  pure def canBeScheduled(s: State): bool =
    not(s.closed) and (if (s.suspendCount == 0) s.queue > 0 or s.sys.length() > 0 else s.sys.length() > 0)

  // Dispatcher.registerForExecution with setAsScheduled as one compare-and-set.
  pure def registered(s: State): State =
    if (canBeScheduled(s) and not(s.scheduled)) { ...s, scheduled: true, submitted: s.submitted + 1 } else s

  def phaseOf(w: str): WorkerPhase = workers.get(w).phase
  def countIn(phases: Set[WorkerPhase]): int = WORKERS.filter(w => phases.contains(phaseOf(w))).size()
  def active: int = countIn(Set(Running, Invoking, Finishing))
  def canInvoke(w: str): bool =
    not(st.closed) and st.suspendCount == 0 and st.queue > 0 and workers.get(w).batch < THROUGHPUT
  def hasWork: bool = (st.suspendCount == 0 and st.queue > 0) or st.sys.length() > 0
  def someoneWillRegister: bool =
    SENDERS.exists(s => Set(SRegistering, SChecked, SEnqueuing).contains(senders.get(s).phase))
      or countIn(Set(Rescheduling)) > 0

  // ---- invariants --------------------------------------------------------

  // One actor is never run by two threads at once.
  val atMostOneRunner = active <= 1
  // The mailbox is in the pool at most once, and the scheduled bit says
  // exactly whether it is in the pool or being run.
  val scheduledMatchesTheRun =
    st.submitted + active <= 1 and (not(st.closed) implies (st.scheduled == (st.submitted + active == 1)))
  // An open mailbox with work is scheduled, or somebody is about to schedule it.
  val noStrandedMessage =
    (not(st.closed) and not(st.scheduled) and hasWork) implies someoneWillRegister
  // `receive` never runs while the actor is suspended or after it is closed.
  val noInvokeWhileSuspended = WORKERS.forall(w => phaseOf(w) == Invoking implies st.suspendCount == 0)
  val noInvokeAfterClose = WORKERS.forall(w => phaseOf(w) == Invoking implies not(st.closed))
  // A closed mailbox holds nothing: what it held went to dead letters.
  val closedHoldsNothing = st.closed implies (st.queue == 0 and st.sys.length() == 0)
  // A run hands back the thread after THROUGHPUT messages.
  val throughputRespected = WORKERS.forall(w => workers.get(w).batch <= THROUGHPUT)
  // Every message sent is processed, queued, being handled, or a dead letter.
  val messagesConserved =
    st.sent == st.processed + st.queue + st.deadLettered + countIn(Set(Invoking))
      + SENDERS.filter(s => senders.get(s).phase == SEnqueuing).size()

  val invariant = atMostOneRunner and scheduledMatchesTheRun and noStrandedMessage and noInvokeWhileSuspended
    and noInvokeAfterClose and closedHoldsNothing and throughputRespected and messagesConserved

  // ---- initial states ----------------------------------------------------

  action initWith(md: Mode): bool = all {
    mode' = md,
    st' = {
      closed: false, scheduled: false, suspendCount: 0, queue: 0, sys: [], submitted: 0,
      controlLeft: CONTROL_BUDGET, pendingDecision: 0, sent: 0, processed: 0, deadLettered: 0,
    },
    workers' = WORKERS.mapBy(_ => { phase: Free, batch: 0 }),
    senders' = SENDERS.mapBy(_ => { phase: SReady, left: SENDS_PER_SENDER }),
  }

  action init = initWith(Fixed)

  // ---- senders -----------------------------------------------------------

  action setSender(s: str, phase: SenderPhase, left: int, after: State): bool = all {
    senders' = senders.set(s, { phase: phase, left: left }),
    st' = after,
    workers' = workers,
    mode' = mode,
  }

  // ActorRef.tell. The fixed mailbox checks `closed` and enqueues under one
  // lock, so a message to a closed mailbox goes to dead letters instead.
  action send(s: str): bool = {
    val sd = senders.get(s)
    all {
      sd.phase == SReady,
      sd.left > 0,
      if (st.closed) setSender(s, SReady, sd.left - 1, { ...st, sent: st.sent + 1, deadLettered: st.deadLettered + 1 })
      else if (mode == LateEnqueue) setSender(s, SEnqueuing, sd.left - 1, { ...st, sent: st.sent + 1 })
      else setSender(s, SRegistering, sd.left - 1, { ...st, sent: st.sent + 1, queue: st.queue + 1 }),
    }
  }

  // LateEnqueue only: the second half of a send, which no longer looks at `closed`.
  action enqueueLate(s: str): bool = all {
    senders.get(s).phase == SEnqueuing,
    setSender(s, SRegistering, senders.get(s).left, { ...st, queue: st.queue + 1 }),
  }

  // registerForExecution after the enqueue.
  action register(s: str): bool = all {
    senders.get(s).phase == SRegistering,
    if (mode == CheckThenAct)
      setSender(s, if (canBeScheduled(st) and not(st.scheduled)) SChecked else SReady, senders.get(s).left, st)
    else setSender(s, SReady, senders.get(s).left, registered(st)),
  }

  // CheckThenAct only: set the bit and submit, without looking again.
  action act(s: str): bool = all {
    senders.get(s).phase == SChecked,
    setSender(s, SReady, senders.get(s).left, { ...st, scheduled: true, submitted: st.submitted + 1 }),
  }

  // ---- control (another actor or the system) -----------------------------

  action setState(after: State): bool = all {
    st' = after,
    workers' = workers,
    senders' = senders,
    mode' = mode,
  }

  // systemEnqueue followed by registerForExecution. Under ExternalSuspend a
  // suspend changes the count directly instead of going through the runner.
  action control(k: SysMsg): bool = all {
    st.controlLeft > 0,
    not(st.closed),
    val spent = { ...st, controlLeft: st.controlLeft - 1 }
    if (mode == ExternalSuspend and k == Suspend) setState({ ...spent, suspendCount: spent.suspendCount + 1 })
    else setState(registered({ ...spent, sys: spent.sys.append(k) })),
  }

  // The parent's supervisor decides to resume the failed child.
  action decide: bool = all {
    st.pendingDecision > 0,
    not(st.closed),
    setState(registered({ ...st, pendingDecision: st.pendingDecision - 1, sys: st.sys.append(Resume) })),
  }

  // ---- workers -----------------------------------------------------------

  action setWorker(w: str, phase: WorkerPhase, batch: int, after: State): bool = all {
    workers' = workers.set(w, { phase: phase, batch: batch }),
    st' = after,
    senders' = senders,
    mode' = mode,
  }

  // A pool thread takes the submitted mailbox and starts Mailbox.run.
  action take(w: str): bool = all {
    phaseOf(w) == Free,
    st.submitted > 0,
    setWorker(w, Running, 0, { ...st, submitted: st.submitted - 1 }),
  }

  // processAllSystemMessages, one message at a time. Terminate closes the
  // mailbox and moves what it holds to dead letters (Mailbox.cleanUp).
  action processSystem(w: str): bool = all {
    phaseOf(w) == Running,
    not(st.closed),
    st.sys.length() > 0,
    val rest = { ...st, sys: st.sys.tail() }
    val after = match st.sys.head() {
      | Suspend => { ...rest, suspendCount: rest.suspendCount + 1 }
      | Resume => { ...rest, suspendCount: if (rest.suspendCount > 0) rest.suspendCount - 1 else 0 }
      | Terminate => {
          ...rest,
          closed: true,
          scheduled: false,
          queue: 0,
          sys: [],
          deadLettered: rest.deadLettered + rest.queue,
        }
    }
    setWorker(w, Running, workers.get(w).batch, after),
  }

  // processMailbox: dequeue one message and hand it to `receive`.
  action startInvoke(w: str): bool = all {
    phaseOf(w) == Running,
    canInvoke(w),
    setWorker(w, Invoking, workers.get(w).batch, { ...st, queue: st.queue - 1 }),
  }

  action endInvoke(w: str): bool = all {
    phaseOf(w) == Invoking,
    setWorker(w, Running, workers.get(w).batch + 1, { ...st, processed: st.processed + 1 }),
  }

  // `receive` failed: the actor suspends itself and tells its parent
  // (FaultHandling.handleInvokeFailure), and the run goes on with what is left.
  action failInvoke(w: str): bool = all {
    phaseOf(w) == Invoking,
    setWorker(w, Running, workers.get(w).batch + 1, {
      ...st,
      processed: st.processed + 1,
      suspendCount: st.suspendCount + 1,
      pendingDecision: st.pendingDecision + 1,
    }),
  }

  // The run has nothing more it may do: no system message is waiting, and no
  // message can be handled (none queued, suspended, or the budget is spent).
  action decideFinish(w: str): bool = all {
    phaseOf(w) == Running,
    st.closed or (not(canInvoke(w)) and st.sys.length() == 0),
    setWorker(w, Finishing, workers.get(w).batch, st),
  }

  // setAsIdle. The fixed run asks to be scheduled again; LostWakeup does not.
  action finishIdle(w: str): bool = all {
    phaseOf(w) == Finishing,
    setWorker(w, if (mode == LostWakeup) Free else Rescheduling, 0, { ...st, scheduled: false }),
  }

  // registerForExecution(this, false, false) at the end of Mailbox.run.
  action finishRegister(w: str): bool = all {
    phaseOf(w) == Rescheduling,
    setWorker(w, Free, 0, registered(st)),
  }

  action step = {
    nondet s = SENDERS.oneOf()
    nondet w = WORKERS.oneOf()
    nondet k = Set(Suspend, Resume, Terminate).oneOf()
    any {
      send(s), enqueueLate(s), register(s), act(s),
      control(k), decide,
      take(w), processSystem(w), startInvoke(w), endInvoke(w), failInvoke(w),
      decideFinish(w), finishIdle(w), finishRegister(w),
    }
  }

  // ---- runs: the fixed mailbox -------------------------------------------

  // A message that arrives while the run is finishing is picked up by the
  // run's own second registration.
  run lateMessageIsRescheduledTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(take("w1"))
      .then(startInvoke("w1")).then(endInvoke("w1")).then(decideFinish("w1"))
      .then(send("s2")).then(register("s2")).then(finishIdle("w1")).then(finishRegister("w1"))
      .expect(st.scheduled and st.submitted == 1 and noStrandedMessage)

  // Two senders race to schedule; the compare-and-set lets one submission through.
  run racingSendersSubmitOnceTest =
    initWith(Fixed).then(send("s1")).then(send("s2")).then(register("s1")).then(register("s2"))
      .expect(st.submitted == 1 and st.scheduled)

  run secondWorkerFindsNothingToTakeTest =
    initWith(Fixed).then(send("s1")).then(send("s2")).then(register("s1")).then(register("s2"))
      .then(take("w1")).then(take("w2")).fail()

  // After THROUGHPUT messages the run must hand the thread back.
  run throughputYieldsTheThreadTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(send("s1")).then(register("s1"))
      .then(send("s2")).then(register("s2")).then(take("w1"))
      .then(startInvoke("w1")).then(endInvoke("w1")).then(startInvoke("w1")).then(endInvoke("w1"))
      .then(startInvoke("w1")).fail()

  run throughputReschedulesTheRestTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(send("s1")).then(register("s1"))
      .then(send("s2")).then(register("s2")).then(take("w1"))
      .then(startInvoke("w1")).then(endInvoke("w1")).then(startInvoke("w1")).then(endInvoke("w1"))
      .then(decideFinish("w1")).then(finishIdle("w1")).then(finishRegister("w1"))
      .then(take("w2")).then(startInvoke("w2")).then(endInvoke("w2"))
      .expect(st.processed == 3 and st.queue == 0)

  // A suspend from outside waits for the runner; `receive` is not interrupted.
  run suspendWaitsForTheRunnerTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(take("w1")).then(startInvoke("w1"))
      .then(control(Suspend))
      .expect(st.suspendCount == 0 and noInvokeWhileSuspended)

  run suspendedActorHandlesNoMessageTest =
    initWith(Fixed).then(control(Suspend)).then(take("w1")).then(processSystem("w1"))
      .then(send("s1")).then(register("s1")).then(startInvoke("w1")).fail()

  // A failure suspends the actor until the parent's Resume arrives.
  run failureWaitsForTheParentTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(send("s1")).then(register("s1"))
      .then(take("w1")).then(startInvoke("w1")).then(failInvoke("w1"))
      .then(decideFinish("w1")).then(finishIdle("w1")).then(finishRegister("w1"))
      .expect(not(st.scheduled) and st.queue == 1 and noStrandedMessage)

  run resumeAfterFailureHandlesTheRestTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(send("s1")).then(register("s1"))
      .then(take("w1")).then(startInvoke("w1")).then(failInvoke("w1"))
      .then(decideFinish("w1")).then(finishIdle("w1")).then(finishRegister("w1"))
      .then(decide).then(take("w2")).then(processSystem("w2")).then(startInvoke("w2")).then(endInvoke("w2"))
      .expect(st.processed == 2 and st.queue == 0 and st.suspendCount == 0)

  // A message to a closed mailbox is a dead letter.
  run sendAfterCloseIsADeadLetterTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(take("w1")).then(control(Terminate))
      .then(processSystem("w1")).then(send("s2"))
      .expect(st.closed and st.deadLettered == 2 and closedHoldsNothing and messagesConserved)

  // ---- runs: the mistakes (negative controls) ----------------------------

  run lostWakeupStrandsAMessageTest =
    initWith(LostWakeup).then(send("s1")).then(register("s1")).then(take("w1"))
      .then(startInvoke("w1")).then(endInvoke("w1")).then(decideFinish("w1"))
      .then(send("s2")).then(register("s2")).then(finishIdle("w1"))
      .expect(not(noStrandedMessage))

  run checkThenActRunsTheActorTwiceTest =
    initWith(CheckThenAct).then(send("s1")).then(send("s2")).then(register("s1")).then(register("s2"))
      .then(act("s1")).then(act("s2")).then(take("w1")).then(take("w2"))
      .expect(not(atMostOneRunner) and not(scheduledMatchesTheRun))

  run externalSuspendInterruptsReceiveTest =
    initWith(ExternalSuspend).then(send("s1")).then(register("s1")).then(take("w1")).then(startInvoke("w1"))
      .then(control(Suspend))
      .expect(not(noInvokeWhileSuspended))

  run lateEnqueueLeavesAMessageInAClosedMailboxTest =
    initWith(LateEnqueue).then(send("s1")).then(enqueueLate("s1")).then(register("s1")).then(take("w1"))
      .then(control(Terminate)).then(send("s2")).then(processSystem("w1")).then(enqueueLate("s2"))
      .expect(not(closedHoldsNothing))
}
