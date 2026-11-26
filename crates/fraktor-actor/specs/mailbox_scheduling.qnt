// The mailbox of one actor and the dispatcher that schedules it on a pool of
// worker threads (docs/design/issue-234-multithread-executor.md, section 5).
//
// It mirrors Pekko's Mailbox status (dispatch/Mailbox.scala: a closed bit, a
// scheduled bit and a suspend count) and the run loop of Mailbox.run with
// Dispatcher.registerForExecution. A sender enqueues and then asks the
// dispatcher to schedule the mailbox. Every reservation takes two steps, as
// in Pekko and in the kernel's MailboxStatus (design sections 3.8 and 3.10):
// it first reads whether the mailbox may be scheduled
// (Mailbox.canBeScheduledForExecution), then sets the scheduled bit with one
// fetch_or that looks at the closed and scheduled bits only (Pekko's
// shouldScheduleMask). Only the caller that turned the bit on in an open
// mailbox submits it to the pool, and the worker that takes the submission
// is the only one that runs the actor. Senders, the senders of system
// messages, the parent's decision and the end of a run all reserve this way.
// Between the two steps the work the caller saw may be gone, so a run may be
// submitted with nothing to do; Pekko has the same wasted run, and the user
// never sees it (submittedWithWorkOrStaleRead below).
//
// A run handles at most THROUGHPUT messages, applies the system messages
// itself (suspend, resume, terminate), clears the scheduled bit, and then
// asks to be scheduled again in case a message arrived while the bit was
// still set.
//
// The status word also holds a "has system messages" flag (design section
// 3.8, idea 5), so that a run looks at one bit, not at the system queue,
// before each normal message. The sender of a system message enqueues it and
// raises the flag inside the gate's lock, so nobody that looks at the queue
// sees the message before the flag. The run lowers the flag first and then
// empties the queue; the other order leaves a message that arrives in
// between in the queue with the flag down (ClearFlagAfterDrain below).
// Reservations read the queues themselves, not the flag: the flag may stay up
// after its message has been handled, and reading it would submit runs that
// have nothing to do.
//
// Terminate stands for the whole stop of an actor that has no children: the
// run that handles it reaches finishTerminate, detaches the actor, and closes
// the mailbox (FaultHandling.finishTerminate, AbstractDispatcher.unregister).
// An actor with children stays open and suspended until its last child is
// gone; that state belongs to actor_termination.qnt (design section 5.4).
// Closing moves every normal message and every system message the mailbox
// still holds to dead letters, and a system message that arrives later goes
// there too (Mailbox.processAllSystemMessages, Mailbox.systemEnqueue). Pekko
// needs this for DeathWatch: a Watch that reaches a closed mailbox is answered
// from dead letters with a DeathWatchNotification.
//
// The modes other than Fixed are the mistakes the design avoids. They are
// reached only through initWith in the runs below, so `init` (and the CI's
// `quint run --invariant=invariant`) explores the fixed mailbox only.
//   LostWakeup      : the run clears the scheduled bit and does not ask again.
//   CheckThenAct    : scheduling reads the bit and then sets it and submits
//                     without looking at what the bit was.
//   ExternalSuspend : a suspend from outside changes the count directly,
//                     while the runner may be inside `receive`.
//   LateEnqueue     : a sender checks `closed` and enqueues in two steps.
//   DropSystemOnClose : closing throws away the system messages that are left,
//                     as the first version of this model did (design review H2).
//   MessagesBeforeSystem : a run takes the next normal message without first
//                     handling the system messages that are waiting.
//   PartialDrain    : after a drain, the run starts on system messages that
//                     arrived later but takes a normal message before it has
//                     handled all of them (Devin's review on PR #236).
//   ClearFlagAfterDrain : the run empties the system queue first and lowers
//                     the flag afterwards (design section 3.8, idea 5).
//   CloseDuringRun  : the executor closes the mailbox from outside while a run
//                     still holds the actor (stage 3, design section 11.11).
//   KeepQueuedRun   : closing from outside leaves the run that was already
//                     submitted in the pool (stage 3).
//   PanicKeepsScheduled : a run that unwinds puts the actor back but leaves
//                     the scheduled bit set (design 11.6 L5).
//   PanicDropsSysFlag : a run that unwinds in the middle of the system
//                     messages does not raise the flag again (design 11.6 L5).
//   PanicAsRecoverable : a panic with no catcher is treated as if the system
//                     could go on: nothing owes the system's termination, so
//                     a Resume lost to the panic leaves the actor suspended for
//                     good (the first stage 3 design, design review High 1).
//   SilentSystemFailure : a system message whose handling fails under the
//                     catcher is dropped without its effect, and neither
//                     suspends the actor nor tells the parent (design review
//                     High 1).
//
// Stage 3 of #234 (design section 11.11) added the transitions below. From
// stage 4a (design section 11.12) the stop of an actor is the Terminate system
// message, which the actor's own run handles (OSystem(Terminate)), and
// closeOutside stands for the termination of the system only:
//   closeOutside : the system terminates (Executor::stop_all) and closes the
//                  mailbox outside every run. The system terminates between
//                  runs, never while a run holds the actor, and it takes the
//                  run that is still waiting in the pool out with it.
//   yieldRun     : a run gives the thread back right after a message whose
//                  handling left work that needs the actor in its slot (a
//                  failure, Kill); the executor does that work after the run
//                  returns (stages 3 to 4a; stage 4b makes it Failed).
//   panicInvoke, panicSystem : `receive` or a system message panics with no
//                  catcher. This is Pekko's fatal error, which escapes
//                  ActorCell.invoke: the guard does what the `finally` of
//                  Mailbox.run does (puts the actor back, raises the flag again
//                  when system messages wait, clears the scheduled bit and
//                  reserves the mailbox again), the message is lost, and the
//                  system owes its termination (Pekko's uncaught exception
//                  handler terminates the system). closeOutside stands for that
//                  termination too.
//   failSystem   : a system message fails under the catcher (Pekko's NonFatal,
//                  caught for each message in systemInvoke). Its effect on
//                  the mailbox stays, the actor suspends itself and tells its
//                  parent (FaultHandling.handleInvokeFailure), and the run goes
//                  on with the rest of the system queue. Terminate never takes
//                  this path: a failing postStop is published and the stop
//                  completes (FaultHandling.finishTerminate).
//
// A run handles every waiting system message before it takes a normal message,
// at its start and again after each normal message (Mailbox.run calls
// processAllSystemMessages, then processMailbox, which calls it again after
// each message). A system message that arrives after that drain may still be
// overtaken by the one normal message taken next, but never by two. Once the
// run starts on a system message, it handles the whole queue before the next
// normal message (processAllSystemMessages loops until the queue is empty).
//
// CI runs 20 steps (scripts/quint-check-specs.sh). For deeper runs pass
// --backend=typescript: the default Rust backend can fail on this spec past
// about 25 steps with "invalid type: null, expected usize" (design 11.8 L1):
//   quint run --backend=typescript --invariant=invariant --max-steps=60 \
//     --max-samples=20000 crates/fraktor-actor/specs/mailbox_scheduling.qnt
module mailbox_scheduling {
  pure val SENDERS = Set("s1", "s2")
  pure val WORKERS = Set("w1", "w2")
  // Who sends system messages from outside the actor (another actor or the system).
  pure val CONTROLLERS = Set("c1", "c2")
  // The parent's supervisor, which answers a failure with Resume.
  pure val PARENT = "p"
  pure val SENDS_PER_SENDER = 2
  pure val THROUGHPUT = 2
  pure val CONTROL_BUDGET = 2

  type Mode = Fixed | LostWakeup | CheckThenAct | ExternalSuspend | LateEnqueue | DropSystemOnClose
    | MessagesBeforeSystem | PartialDrain | ClearFlagAfterDrain | CloseDuringRun | KeepQueuedRun
    | PanicKeepsScheduled | PanicDropsSysFlag | PanicAsRecoverable | SilentSystemFailure
  type SysMsg = Suspend | Resume | Terminate
  // What an outside caller asks for: a system message, or the termination of
  // the system, closeOutside (in stage 3 also the executor's synchronous stop).
  type Outside = OSystem(SysMsg) | OStop
  // mark: how many normal messages had been taken when the system message arrived.
  type SysEntry = { kind: SysMsg, mark: int }
  // SRegistering: enqueued, about to read whether the mailbox may be scheduled.
  // SFetching: read that it may, about to fetch_or the scheduled bit.
  // SChecked: under CheckThenAct, the sender saw the bit clear and will set it.
  // SEnqueuing: under LateEnqueue, the sender saw the mailbox open and will enqueue.
  type SenderPhase = SReady | SRegistering | SFetching | SChecked | SEnqueuing
  // The same two steps for a sender of system messages and for the parent.
  type RegPhase = RIdle | RReading | RFetching
  // Running, Invoking and Finishing hold the scheduled bit. Rescheduling has
  // cleared it and is about to read whether to ask again; Refetching read
  // that it should and is about to fetch_or.
  type WorkerPhase = Free | Running | Invoking | Finishing | Rescheduling | Refetching
  // DDraining: the run lowered the flag and is emptying the system queue.
  // DEmptied: under ClearFlagAfterDrain, the run saw the queue empty and has
  // not lowered the flag yet.
  type DrainStep = DNone | DDraining | DEmptied

  type State = {
    closed: bool,
    scheduled: bool,
    suspendCount: int,
    // The "has system messages" bit of the status word.
    sysFlag: bool,
    queue: int,
    sys: List[SysEntry],
    submitted: int,
    controlLeft: int,
    pendingDecision: int,
    sent: int,
    processed: int,
    deadLettered: int,
    // System messages: sent, applied by the runner, and moved to dead letters.
    sysSent: int,
    sysProcessed: int,
    sysDeadLettered: int,
    // How many normal messages runs have taken (started handling) so far.
    started: int,
    // Ghost: suspends asked for from outside that no outside resume has answered yet.
    externalSuspendsPending: int,
    // Ghost: the submission in the pool was made by a caller that read work in
    // its first step, and the work was gone by its fetch_or.
    staleSubmission: bool,
    // A panic with no catcher has escaped a run (Pekko's fatal error). The
    // system owes its termination, which closes the mailbox.
    fatal: bool,
    // Ghost: the normal and system messages a fatal panic took with it. They
    // count in the conservation of messages, and only a fatal panic makes them.
    lostToFatal: int,
    sysLostToFatal: int,
  }
  // drained: the run has handled every waiting system message since it last took a normal message.
  // drainMark: how many system messages had been handled when the run last saw the queue empty.
  // sawWork: in Refetching, whether the run's read found work (ghost).
  type Worker = { phase: WorkerPhase, batch: int, drained: bool, drainMark: int, drain: DrainStep, sawWork: bool }
  // sawWork: in SFetching, whether the sender's read found work (ghost).
  type Sender = { phase: SenderPhase, left: int, sawWork: bool }
  type Registrant = { phase: RegPhase, sawWork: bool }

  var mode: Mode
  var st: State
  var workers: str -> Worker
  var senders: str -> Sender
  // The controllers and the parent.
  var regs: str -> Registrant

  // Mailbox.canBeScheduledForExecution: an open mailbox runs for any message,
  // a suspended one only for a system message, a closed one never. It reads
  // the queues, not the flag.
  pure def canBeScheduled(s: State): bool =
    not(s.closed) and (if (s.suspendCount == 0) s.queue > 0 or s.sys.length() > 0 else s.sys.length() > 0)

  // What a run may do. Kept apart from canBeScheduled, so that a wrong
  // scheduling rule cannot also weaken the checks of itself.
  pure def hasWorkIn(s: State): bool = (s.suspendCount == 0 and s.queue > 0) or s.sys.length() > 0

  // The second step of a reservation: fetch_or(SCHEDULED). The caller submits
  // only when neither the closed nor the scheduled bit was set before.
  pure def fetchedOr(s: State, sawWork: bool): State =
    if (not(s.closed) and not(s.scheduled))
      { ...s, scheduled: true, submitted: s.submitted + 1, staleSubmission: sawWork and not(hasWorkIn(s)) }
    else { ...s, scheduled: true }

  def phaseOf(w: str): WorkerPhase = workers.get(w).phase
  def countIn(phases: Set[WorkerPhase]): int = WORKERS.filter(w => phases.contains(phaseOf(w))).size()
  def active: int = countIn(Set(Running, Invoking, Finishing))
  def canInvoke(w: str): bool =
    not(st.closed) and st.suspendCount == 0 and st.queue > 0 and workers.get(w).batch < THROUGHPUT
  def hasWork: bool = hasWorkIn(st)
  def someoneWillRegister: bool =
    SENDERS.exists(s => Set(SRegistering, SFetching, SChecked, SEnqueuing).contains(senders.get(s).phase))
      or regs.keys().exists(r => regs.get(r).phase != RIdle)
      or countIn(Set(Rescheduling, Refetching)) > 0

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
      + SENDERS.filter(s => senders.get(s).phase == SEnqueuing).size() + st.lostToFatal

  // Every system message sent is applied, still queued, a dead letter, or
  // lost to a fatal panic.
  val systemMessagesConserved =
    st.sysSent == st.sysProcessed + st.sys.length() + st.sysDeadLettered + st.sysLostToFatal
  // Only a fatal panic loses a message. A failure under the catcher handles
  // the message it failed on (failInvoke, failSystem) and loses nothing.
  val lossOnlyWhenFatal = (st.lostToFatal > 0 or st.sysLostToFatal > 0) implies st.fatal
  // I13 before stage 2: a submitted mailbox always has something its run may
  // do. The two-step reservation breaks it in a way Pekko allows (design
  // section 3.10), so it is not part of `invariant`; staleReadSubmitsARunTest
  // shows the case.
  val submittedOnlyWithWork = st.submitted > 0 implies (not(st.closed) and hasWork)
  // I13: a submitted mailbox has something its run may do, unless the caller
  // that submitted it read work in its first step and the work was gone by its
  // fetch_or. A suspended actor is never submitted for normal messages alone.
  val submittedWithWorkOrStaleRead = st.submitted > 0 implies (not(st.closed) and (hasWork or st.staleSubmission))
  // When nothing is in flight (no sender between its steps, no run, no
  // submission, no decision owed by the parent, no termination owed after a
  // fatal panic), an open mailbox has handled everything it may handle, and it
  // is suspended only if somebody asked for it. A failure under the catcher is
  // in flight until the parent has answered it, so it is no exception. After a
  // fatal panic the system promises only to terminate, as Pekko does: the
  // mailbox is not settled until it is closed.
  def settled: bool =
    SENDERS.forall(s => senders.get(s).phase == SReady) and countIn(Set(Free)) == WORKERS.size()
      and regs.keys().forall(r => regs.get(r).phase == RIdle)
      and st.submitted == 0 and st.pendingDecision == 0
      and (not(st.fatal) or st.closed)
  val settledMeansIdle =
    (settled and not(st.closed)) implies
      (st.sys.length() == 0 and (st.suspendCount == 0 implies st.queue == 0)
        and (st.suspendCount > 0 implies st.externalSuspendsPending > 0))
  // A system message is overtaken by at most one normal message: the one a run
  // took right after its last drain, before the system message arrived.
  val systemBeforeNormal = st.sys.foldl(true, (ok, e) => ok and st.started <= e.mark + 1)
  // No system message is handled between the drain and the normal message
  // taken after it: once the run starts on the system messages again, it has
  // to drain them all before the next normal message.
  val noSystemSinceDrain =
    WORKERS.forall(w => phaseOf(w) == Invoking implies st.sysProcessed == workers.get(w).drainMark)
  // A system message in an open mailbox is never hidden from the run: the
  // flag is up, or a run lowered it and is still emptying the queue.
  val noHiddenSystemMessage =
    (not(st.closed) and st.sys.length() > 0) implies (st.sysFlag or WORKERS.exists(w => workers.get(w).drain == DDraining))

  val invariant = atMostOneRunner and scheduledMatchesTheRun and noStrandedMessage and noInvokeWhileSuspended
    and noInvokeAfterClose and closedHoldsNothing and throughputRespected and messagesConserved
    and systemMessagesConserved and submittedWithWorkOrStaleRead and settledMeansIdle and systemBeforeNormal
    and noSystemSinceDrain and noHiddenSystemMessage and lossOnlyWhenFatal

  // ---- initial states ----------------------------------------------------

  action initWith(md: Mode): bool = all {
    mode' = md,
    st' = {
      closed: false, scheduled: false, suspendCount: 0, sysFlag: false, queue: 0, sys: [], submitted: 0,
      controlLeft: CONTROL_BUDGET, pendingDecision: 0, sent: 0, processed: 0, deadLettered: 0,
      sysSent: 0, sysProcessed: 0, sysDeadLettered: 0, started: 0, externalSuspendsPending: 0,
      staleSubmission: false, fatal: false, lostToFatal: 0, sysLostToFatal: 0,
    },
    workers' = WORKERS.mapBy(_ => { phase: Free, batch: 0, drained: false, drainMark: 0, drain: DNone, sawWork: false }),
    senders' = SENDERS.mapBy(_ => { phase: SReady, left: SENDS_PER_SENDER, sawWork: false }),
    regs' = CONTROLLERS.union(Set(PARENT)).mapBy(_ => { phase: RIdle, sawWork: false }),
  }

  action init = initWith(Fixed)

  // ---- senders -----------------------------------------------------------

  action setSender(s: str, phase: SenderPhase, left: int, sawWork: bool, after: State): bool = all {
    senders' = senders.set(s, { phase: phase, left: left, sawWork: sawWork }),
    st' = after,
    workers' = workers,
    regs' = regs,
    mode' = mode,
  }

  // ActorRef.tell. The fixed mailbox checks `closed` and enqueues under one
  // lock, so a message to a closed mailbox goes to dead letters instead.
  action send(s: str): bool = {
    val sd = senders.get(s)
    all {
      sd.phase == SReady,
      sd.left > 0,
      if (st.closed)
        setSender(s, SReady, sd.left - 1, false, { ...st, sent: st.sent + 1, deadLettered: st.deadLettered + 1 })
      else if (mode == LateEnqueue) setSender(s, SEnqueuing, sd.left - 1, false, { ...st, sent: st.sent + 1 })
      else setSender(s, SRegistering, sd.left - 1, false, { ...st, sent: st.sent + 1, queue: st.queue + 1 }),
    }
  }

  // LateEnqueue only: the second half of a send, which no longer looks at `closed`.
  action enqueueLate(s: str): bool = all {
    senders.get(s).phase == SEnqueuing,
    setSender(s, SRegistering, senders.get(s).left, false, { ...st, queue: st.queue + 1 }),
  }

  // registerForExecution after the enqueue, first step: canBeScheduledForExecution.
  action register(s: str): bool = all {
    senders.get(s).phase == SRegistering,
    if (mode == CheckThenAct)
      setSender(s, if (canBeScheduled(st) and not(st.scheduled)) SChecked else SReady, senders.get(s).left, false, st)
    else if (canBeScheduled(st)) setSender(s, SFetching, senders.get(s).left, hasWork, st)
    else setSender(s, SReady, senders.get(s).left, false, st),
  }

  // Second step: setAsScheduled as one fetch_or.
  action fetchOr(s: str): bool = all {
    senders.get(s).phase == SFetching,
    setSender(s, SReady, senders.get(s).left, false, fetchedOr(st, senders.get(s).sawWork)),
  }

  // CheckThenAct only: set the bit and submit, without looking at what it was.
  action act(s: str): bool = all {
    senders.get(s).phase == SChecked,
    setSender(s, SReady, senders.get(s).left, false, { ...st, scheduled: true, submitted: st.submitted + 1 }),
  }

  // ---- control (another actor, the system, or the parent) ----------------

  action setState(after: State): bool = all {
    st' = after,
    workers' = workers,
    senders' = senders,
    regs' = regs,
    mode' = mode,
  }

  action setReg(r: str, phase: RegPhase, sawWork: bool, after: State): bool = all {
    regs' = regs.set(r, { phase: phase, sawWork: sawWork }),
    st' = after,
    workers' = workers,
    senders' = senders,
    mode' = mode,
  }

  // systemEnqueue: in the gate's lock, enqueue and raise the flag; then
  // registerForExecution in two steps (regRead, regFetchOr). A system message
  // to a closed mailbox is a dead letter (Mailbox.systemEnqueue hands it to the
  // dead-letter mailbox). Under ExternalSuspend a suspend changes the count
  // directly instead of going through the runner.
  action control(c: str, k: SysMsg): bool = all {
    regs.get(c).phase == RIdle,
    st.controlLeft > 0,
    val spent = {
      ...st,
      controlLeft: st.controlLeft - 1,
      sysSent: st.sysSent + 1,
      externalSuspendsPending:
        if (k == Suspend) st.externalSuspendsPending + 1
        else if (k == Resume and st.externalSuspendsPending > 0) st.externalSuspendsPending - 1
        else st.externalSuspendsPending,
    }
    if (st.closed)
      setState(if (mode == DropSystemOnClose) spent else { ...spent, sysDeadLettered: spent.sysDeadLettered + 1 })
    else if (mode == ExternalSuspend and k == Suspend)
      setState({ ...spent, suspendCount: spent.suspendCount + 1, sysProcessed: spent.sysProcessed + 1 })
    else setReg(c, RReading, false, { ...spent, sys: spent.sys.append({ kind: k, mark: st.started }), sysFlag: true }),
  }

  // The parent's supervisor decides to resume the failed child.
  action decide: bool = all {
    st.pendingDecision > 0,
    not(st.closed),
    regs.get(PARENT).phase == RIdle,
    setReg(PARENT, RReading, false, {
      ...st,
      pendingDecision: st.pendingDecision - 1,
      sys: st.sys.append({ kind: Resume, mark: st.started }),
      sysFlag: true,
      sysSent: st.sysSent + 1,
    }),
  }

  action regRead(r: str): bool = all {
    regs.get(r).phase == RReading,
    if (canBeScheduled(st)) setReg(r, RFetching, hasWork, st) else setReg(r, RIdle, false, st),
  }

  action regFetchOr(r: str): bool = all {
    regs.get(r).phase == RFetching,
    setReg(r, RIdle, false, fetchedOr(st, regs.get(r).sawWork)),
  }

  // ---- workers -----------------------------------------------------------

  action setWorkerTo(w: str, wk: Worker, after: State): bool = all {
    workers' = workers.set(w, wk),
    st' = after,
    senders' = senders,
    regs' = regs,
    mode' = mode,
  }

  action setWorker(w: str, phase: WorkerPhase, batch: int, after: State): bool =
    setWorkerTo(w, { ...workers.get(w), phase: phase, batch: batch }, after)

  // A pool thread takes the submitted mailbox and starts Mailbox.run.
  action take(w: str): bool = all {
    phaseOf(w) == Free,
    st.submitted > 0,
    setWorkerTo(
      w,
      { ...workers.get(w), phase: Running, batch: 0, drained: false, drain: DNone },
      { ...st, submitted: st.submitted - 1, staleSubmission: false },
    ),
  }

  // The flag is up: lower it, then empty the queue (processSystem, drainDone).
  // Starting on the system messages cancels the earlier drain: the run has to
  // see the queue empty once more before it takes a normal message. Under
  // ClearFlagAfterDrain the flag comes down only after the queue was seen empty.
  action clearSysFlag(w: str): bool = {
    val wk = workers.get(w)
    all {
      phaseOf(w) == Running,
      not(st.closed),
      if (mode == ClearFlagAfterDrain) all {
        wk.drain == DEmptied,
        setWorkerTo(w, { ...wk, drain: DNone }, { ...st, sysFlag: false }),
      } else all {
        st.sysFlag,
        wk.drain == DNone,
        setWorkerTo(w, { ...wk, drain: DDraining, drained: mode == PartialDrain and wk.drained }, { ...st, sysFlag: false }),
      },
    }
  }

  // ClearFlagAfterDrain only: start emptying the queue with the flag still up.
  action beginDrain(w: str): bool = all {
    mode == ClearFlagAfterDrain,
    phaseOf(w) == Running,
    not(st.closed),
    st.sysFlag,
    workers.get(w).drain == DNone,
    setWorkerTo(w, { ...workers.get(w), drain: DDraining, drained: false }, st),
  }

  // processAllSystemMessages, one message at a time. Terminate closes the
  // mailbox and moves what it holds, normal and system messages alike, to dead
  // letters (Mailbox.processAllSystemMessages and Mailbox.cleanUp).
  action processSystem(w: str): bool = all {
    phaseOf(w) == Running,
    not(st.closed),
    workers.get(w).drain == DDraining,
    st.sys.length() > 0,
    val rest = { ...st, sys: st.sys.tail(), sysProcessed: st.sysProcessed + 1 }
    val after = match st.sys.head().kind {
      | Suspend => { ...rest, suspendCount: rest.suspendCount + 1 }
      | Resume => { ...rest, suspendCount: if (rest.suspendCount > 0) rest.suspendCount - 1 else 0 }
      | Terminate => {
          ...rest,
          closed: true,
          scheduled: false,
          queue: 0,
          sys: [],
          deadLettered: rest.deadLettered + rest.queue,
          sysDeadLettered:
            if (mode == DropSystemOnClose) rest.sysDeadLettered else rest.sysDeadLettered + rest.sys.length(),
        }
    }
    setWorker(w, Running, workers.get(w).batch, after),
  }

  // The run has handled every waiting system message, so it may take a normal
  // message: either it emptied the queue, or the flag was down and it did not
  // look at the queue at all.
  action drainDone(w: str): bool = {
    val wk = workers.get(w)
    all {
      phaseOf(w) == Running,
      not(st.closed),
      not(wk.drained),
      if (wk.drain == DDraining) all {
        st.sys.length() == 0,
        if (mode == ClearFlagAfterDrain) setWorkerTo(w, { ...wk, drain: DEmptied }, st)
        else setWorkerTo(w, { ...wk, drain: DNone, drained: true, drainMark: st.sysProcessed }, st),
      } else all {
        wk.drain == DNone,
        not(st.sysFlag),
        setWorkerTo(w, { ...wk, drained: true, drainMark: st.sysProcessed }, st),
      },
    }
  }

  // processMailbox: dequeue one message and hand it to `receive`. The fixed run
  // takes it only right after a drain; MessagesBeforeSystem does not wait.
  action startInvoke(w: str): bool = all {
    phaseOf(w) == Running,
    canInvoke(w),
    workers.get(w).drained or mode == MessagesBeforeSystem,
    setWorkerTo(w, { ...workers.get(w), phase: Invoking, drained: false }, { ...st, queue: st.queue - 1, started: st.started + 1 }),
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

  // The run has nothing more it may do: the flag is down and no drain is under
  // way, and no message can be handled (none queued, suspended, or the budget
  // is spent).
  action decideFinish(w: str): bool = all {
    phaseOf(w) == Running,
    st.closed or (not(canInvoke(w)) and not(st.sysFlag) and workers.get(w).drain == DNone),
    setWorker(w, Finishing, workers.get(w).batch, st),
  }

  // setAsIdle. The fixed run asks to be scheduled again; LostWakeup does not.
  action finishIdle(w: str): bool = all {
    phaseOf(w) == Finishing,
    setWorkerTo(
      w,
      { ...workers.get(w), phase: if (mode == LostWakeup) Free else Rescheduling, batch: 0, drained: false, drain: DNone },
      { ...st, scheduled: false },
    ),
  }

  // registerForExecution(this, false, false) at the end of Mailbox.run, first
  // step: canBeScheduledForExecution.
  action finishRegister(w: str): bool = all {
    phaseOf(w) == Rescheduling,
    if (canBeScheduled(st)) setWorkerTo(w, { ...workers.get(w), phase: Refetching, sawWork: hasWork }, st)
    else setWorkerTo(w, { ...workers.get(w), phase: Free, sawWork: false }, st),
  }

  // Second step: fetch_or. A win is RunOutcome::Again in the kernel.
  action finishFetchOr(w: str): bool = all {
    phaseOf(w) == Refetching,
    setWorkerTo(w, { ...workers.get(w), phase: Free, sawWork: false }, fetchedOr(st, workers.get(w).sawWork)),
  }

  // ---- stage 3 (design section 11.11) ------------------------------------

  // The termination of the system (Executor::stop_all), which is also what a
  // fatal panic owes, and in stage 3 the executor's synchronous stop: it
  // closes the mailbox outside every run, moves what the mailbox held to dead
  // letters, and takes the run still waiting in the pool out of the pool. It
  // never closes a mailbox a worker holds (the system terminates between runs;
  // design 11.11.5 and 11.12); CloseDuringRun does, and KeepQueuedRun leaves
  // the waiting run in the pool.
  action closeOutside: bool = all {
    not(st.closed),
    mode == CloseDuringRun or countIn(Set(Free)) == WORKERS.size(),
    setState({
      ...st,
      closed: true,
      queue: 0,
      sys: [],
      deadLettered: st.deadLettered + st.queue,
      sysDeadLettered: st.sysDeadLettered + st.sys.length(),
      submitted: if (mode == KeepQueuedRun) st.submitted else 0,
      staleSubmission: if (mode == KeepQueuedRun) st.staleSubmission else false,
    }),
  }

  // The run ends early after a message whose handling left work for the
  // executor that needs the actor back in its slot. It ends the way a run that
  // spent its throughput does: with the flag down and no drain under way.
  action yieldRun(w: str): bool = all {
    phaseOf(w) == Running,
    not(st.closed),
    workers.get(w).batch > 0,
    not(st.sysFlag),
    workers.get(w).drain == DNone,
    setWorker(w, Finishing, workers.get(w).batch, st),
  }

  // The guard of MailboxRun::run when the run unwinds, which is the `finally`
  // of Pekko's Mailbox.run: the actor goes back, the flag goes up again when
  // system messages are still queued, and the scheduled bit comes down before
  // the mailbox is reserved again (finishRegister). The panic had no catcher,
  // so it is fatal and the system owes its termination (design 11.11.2).
  // PanicKeepsScheduled leaves the bit set and the thread gone;
  // PanicDropsSysFlag leaves the flag as the drain left it; PanicAsRecoverable
  // goes on as if nothing had happened.
  action unwind(w: str, after: State): bool = setWorkerTo(
    w,
    {
      ...workers.get(w),
      phase: if (mode == PanicKeepsScheduled) Free else Rescheduling,
      batch: 0,
      drained: false,
      drain: DNone,
    },
    {
      ...after,
      scheduled: if (mode == PanicKeepsScheduled) after.scheduled else false,
      sysFlag: if (mode == PanicDropsSysFlag) after.sysFlag else after.sysFlag or after.sys.length() > 0,
      fatal: after.fatal or mode != PanicAsRecoverable,
    },
  )

  // `receive` panics with no catcher: the message it was handling is lost.
  action panicInvoke(w: str): bool = all {
    phaseOf(w) == Invoking,
    unwind(w, { ...st, lostToFatal: st.lostToFatal + 1 }),
  }

  // A system message panics with no catcher while the run empties the system
  // queue: that message is lost with its effect, the rest stay queued.
  action panicSystem(w: str): bool = all {
    phaseOf(w) == Running,
    not(st.closed),
    workers.get(w).drain == DDraining,
    st.sys.length() > 0,
    unwind(w, { ...st, sys: st.sys.tail(), sysLostToFatal: st.sysLostToFatal + 1 }),
  }

  // A system message fails under the catcher (systemInvoke catches each one).
  // Its effect on the mailbox stays, then the actor suspends itself and asks
  // its parent, as failInvoke does, and the drain goes on. Terminate is left
  // out: a failing postStop is published and the stop completes, which is
  // processSystem. SilentSystemFailure drops the message instead.
  action failSystem(w: str): bool = all {
    phaseOf(w) == Running,
    not(st.closed),
    workers.get(w).drain == DDraining,
    st.sys.length() > 0,
    st.sys.head().kind != Terminate,
    val rest = { ...st, sys: st.sys.tail(), sysProcessed: st.sysProcessed + 1 }
    val applied = match st.sys.head().kind {
      | Suspend => rest.suspendCount + 1
      | Resume => if (rest.suspendCount > 0) rest.suspendCount - 1 else 0
      | Terminate => rest.suspendCount
    }
    val after =
      if (mode == SilentSystemFailure) rest
      else { ...rest, suspendCount: applied + 1, pendingDecision: rest.pendingDecision + 1 }
    setWorker(w, Running, workers.get(w).batch, after),
  }

  // The termination is one of the outside operations, not a branch of its
  // own: as its own branch it was enabled at every step of an open mailbox with
  // no run, closed the mailbox early in most traces, and halved the average
  // length of a trace (design 11.11.10).
  action outside(c: str, op: Outside): bool = match op {
    | OSystem(k) => control(c, k)
    | OStop => closeOutside
  }

  action step = {
    nondet s = SENDERS.oneOf()
    nondet w = WORKERS.oneOf()
    nondet c = CONTROLLERS.oneOf()
    nondet r = CONTROLLERS.union(Set(PARENT)).oneOf()
    nondet op = Set(OSystem(Suspend), OSystem(Resume), OSystem(Terminate), OStop).oneOf()
    any {
      send(s), enqueueLate(s), register(s), fetchOr(s), act(s),
      outside(c, op), decide, regRead(r), regFetchOr(r),
      take(w), clearSysFlag(w), beginDrain(w), processSystem(w), drainDone(w), startInvoke(w), endInvoke(w),
      failInvoke(w), decideFinish(w), finishIdle(w), finishRegister(w), finishFetchOr(w),
      yieldRun(w), panicInvoke(w), panicSystem(w), failSystem(w),
    }
  }

  // ---- runs: the fixed mailbox -------------------------------------------

  // A message that arrives while the run is finishing is picked up by the
  // run's own second registration.
  run lateMessageIsRescheduledTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1")).then(decideFinish("w1"))
      .then(send("s2")).then(register("s2")).then(fetchOr("s2"))
      .then(finishIdle("w1")).then(finishRegister("w1")).then(finishFetchOr("w1"))
      .expect(st.scheduled and st.submitted == 1 and noStrandedMessage)

  // Two senders race to schedule; the fetch_or lets one submission through.
  run racingSendersSubmitOnceTest =
    initWith(Fixed).then(send("s1")).then(send("s2")).then(register("s1")).then(register("s2"))
      .then(fetchOr("s1")).then(fetchOr("s2"))
      .expect(st.submitted == 1 and st.scheduled)

  run secondWorkerFindsNothingToTakeTest =
    initWith(Fixed).then(send("s1")).then(send("s2")).then(register("s1")).then(register("s2"))
      .then(fetchOr("s2")).then(fetchOr("s1")).then(take("w1")).then(take("w2")).fail()

  // A sender reads work, the running actor handles it, and the sender's
  // fetch_or then submits a run with nothing to do. Pekko allows this; the
  // I13 of the first version of this model did not (design section 3.10).
  run staleReadSubmitsARunTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(send("s2")).then(register("s2"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1"))
      .then(decideFinish("w1")).then(finishIdle("w1")).then(finishRegister("w1"))
      .then(fetchOr("s2"))
      .expect(st.submitted == 1 and not(hasWork) and st.staleSubmission
        and submittedWithWorkOrStaleRead and not(submittedOnlyWithWork) and invariant)

  // After THROUGHPUT messages the run must hand the thread back.
  run throughputYieldsTheThreadTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s2")).then(register("s2")).then(fetchOr("s2")).then(take("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1")).then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).fail()

  run throughputReschedulesTheRestTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s2")).then(register("s2")).then(fetchOr("s2")).then(take("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1")).then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1"))
      .then(decideFinish("w1")).then(finishIdle("w1")).then(finishRegister("w1")).then(finishFetchOr("w1"))
      .then(take("w2")).then(drainDone("w2")).then(startInvoke("w2")).then(endInvoke("w2"))
      .expect(st.processed == 3 and st.queue == 0)

  // A suspend from outside waits for the runner; `receive` is not interrupted.
  run suspendWaitsForTheRunnerTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(control("c1", Suspend))
      .expect(st.suspendCount == 0 and noInvokeWhileSuspended)

  run suspendedActorHandlesNoMessageTest =
    initWith(Fixed).then(control("c1", Suspend)).then(regRead("c1")).then(regFetchOr("c1")).then(take("w1"))
      .then(clearSysFlag("w1")).then(processSystem("w1"))
      .then(send("s1")).then(register("s1")).then(drainDone("w1")).then(startInvoke("w1")).fail()

  // A failure suspends the actor until the parent's Resume arrives.
  run failureWaitsForTheParentTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(take("w1")).then(drainDone("w1")).then(startInvoke("w1")).then(failInvoke("w1"))
      .then(decideFinish("w1")).then(finishIdle("w1")).then(finishRegister("w1"))
      .expect(not(st.scheduled) and st.queue == 1 and noStrandedMessage)

  run resumeAfterFailureHandlesTheRestTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(take("w1")).then(drainDone("w1")).then(startInvoke("w1")).then(failInvoke("w1"))
      .then(decideFinish("w1")).then(finishIdle("w1")).then(finishRegister("w1"))
      .then(decide).then(regRead(PARENT)).then(regFetchOr(PARENT))
      .then(take("w2")).then(clearSysFlag("w2")).then(processSystem("w2")).then(drainDone("w2"))
      .then(startInvoke("w2")).then(endInvoke("w2"))
      .expect(st.processed == 2 and st.queue == 0 and st.suspendCount == 0)

  // A message to a closed mailbox is a dead letter.
  run sendAfterCloseIsADeadLetterTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(control("c1", Terminate)).then(clearSysFlag("w1")).then(processSystem("w1")).then(send("s2"))
      .expect(st.closed and st.deadLettered == 2 and closedHoldsNothing and messagesConserved)

  // The system messages behind Terminate, such as a Watch, go to dead letters
  // with the rest, so the watcher still hears that the actor is gone.
  run remainingSystemMessagesAreDeadLettersTest =
    initWith(Fixed).then(control("c1", Terminate)).then(control("c2", Resume))
      .then(regRead("c1")).then(regFetchOr("c1")).then(take("w1")).then(clearSysFlag("w1")).then(processSystem("w1"))
      .expect(st.closed and st.sysDeadLettered == 1 and systemMessagesConserved)

  run systemMessageAfterCloseIsADeadLetterTest =
    initWith(Fixed).then(control("c1", Terminate)).then(regRead("c1")).then(regFetchOr("c1")).then(take("w1"))
      .then(clearSysFlag("w1")).then(processSystem("w1")).then(control("c2", Suspend))
      .expect(st.sysDeadLettered == 1 and systemMessagesConserved and closedHoldsNothing)

  // An outside suspend followed by an outside resume lets the actor go on.
  run externalSuspendThenResumeHandlesTheMessageTest =
    initWith(Fixed).then(control("c1", Suspend)).then(regRead("c1")).then(regFetchOr("c1")).then(take("w1"))
      .then(clearSysFlag("w1")).then(processSystem("w1"))
      .then(send("s1")).then(register("s1")).then(control("c2", Resume)).then(processSystem("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1"))
      .expect(st.processed == 1 and st.suspendCount == 0 and st.externalSuspendsPending == 0)

  // A system message that arrives during a run is handled before the next
  // normal message: its flag stops the run from taking one.
  run systemMessageGoesBeforeTheNextMessageTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(take("w1")).then(control("c1", Resume)).then(drainDone("w1")).fail()

  // A system message that arrives while the run empties the queue, after the
  // flag came down, is still handled in the same drain, and nothing is hidden.
  run systemMessageDuringTheDrainIsHandledTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(control("c1", Resume)).then(clearSysFlag("w1")).then(processSystem("w1"))
      .then(control("c2", Resume)).then(processSystem("w1")).then(drainDone("w1"))
      .expect(st.sys.length() == 0 and st.sysProcessed == 2 and noHiddenSystemMessage and invariant)

  // A system message that arrives after the drain may be overtaken by one
  // normal message, but once the run starts on it, the run drains the whole
  // queue before the next normal message (Devin's review on PR #236).
  run startedDrainIsFinishedBeforeTheNextMessageTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1")).then(drainDone("w1"))
      .then(control("c1", Resume)).then(control("c2", Suspend)).then(clearSysFlag("w1")).then(processSystem("w1"))
      .then(startInvoke("w1")).fail()

  // ---- runs: stage 3 ------------------------------------------------------

  // The executor stops an actor whose run waits in the pool: the run goes
  // with the stop, and what the mailbox held goes to dead letters.
  run closeOutsideTakesTheWaitingRunTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(send("s2"))
      .then(closeOutside)
      .expect(st.closed and st.submitted == 0 and st.deadLettered == 2 and invariant)

  // The executor does not close the mailbox of a run in progress.
  run closeOutsideWaitsForTheRunTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(closeOutside).fail()

  // A run that ends early after a message is reserved again for the rest.
  run yieldedRunReschedulesTheRestTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(take("w1")).then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1"))
      .then(yieldRun("w1")).then(finishIdle("w1")).then(finishRegister("w1")).then(finishFetchOr("w1"))
      .expect(st.submitted == 1 and st.scheduled and st.queue == 1 and invariant)

  // A panic with no catcher out of `receive` loses its message and leaves the
  // mailbox reserved for what is left, and the system owes its termination.
  run panicInReceiveReschedulesTheRestTest =
    initWith(Fixed).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(take("w1")).then(drainDone("w1")).then(startInvoke("w1")).then(panicInvoke("w1"))
      .then(finishRegister("w1")).then(finishFetchOr("w1"))
      .expect(st.submitted == 1 and st.scheduled and st.lostToFatal == 1 and st.queue == 1 and st.fatal and invariant)

  // A panic in the middle of the system messages raises the flag again, so
  // the next run handles the rest before any normal message.
  run panicInTheDrainRaisesTheFlagAgainTest =
    initWith(Fixed).then(control("c1", Resume)).then(control("c2", Suspend)).then(regRead("c1")).then(regFetchOr("c1"))
      .then(take("w1")).then(clearSysFlag("w1")).then(panicSystem("w1"))
      .then(finishRegister("w1")).then(finishFetchOr("w1"))
      .expect(st.sysFlag and st.sys.length() == 1 and st.submitted == 1 and invariant)

  // A Resume lost to a panic with no catcher leaves the actor suspended, but
  // the mailbox is not settled: the system owes its termination, which closes
  // it (design review High 1).
  run fatalPanicOwesTheTerminationTest =
    initWith(Fixed).then(control("c1", Suspend)).then(control("c2", Resume))
      .then(regRead("c1")).then(regFetchOr("c1")).then(regRead("c2")).then(regFetchOr("c2"))
      .then(take("w1")).then(clearSysFlag("w1")).then(processSystem("w1")).then(panicSystem("w1"))
      .then(finishRegister("w1"))
      .expect(st.fatal and st.suspendCount == 1 and st.sysLostToFatal == 1 and not(settled) and invariant)
      .then(closeOutside)
      .expect(st.closed and settled and invariant)

  // A Resume that fails under the catcher keeps its effect, and the failure
  // suspends the actor and asks the parent, whose Resume lets it go on.
  run failedSystemMessageAsksTheParentTest =
    initWith(Fixed).then(control("c1", Suspend)).then(control("c2", Resume))
      .then(regRead("c1")).then(regFetchOr("c1")).then(regRead("c2")).then(regFetchOr("c2"))
      .then(take("w1")).then(clearSysFlag("w1")).then(processSystem("w1")).then(failSystem("w1"))
      .then(drainDone("w1")).then(decideFinish("w1")).then(finishIdle("w1")).then(finishRegister("w1"))
      .expect(st.suspendCount == 1 and st.pendingDecision == 1 and not(settled) and invariant)
      .then(decide).then(regRead(PARENT)).then(regFetchOr(PARENT))
      .then(take("w2")).then(clearSysFlag("w2")).then(processSystem("w2")).then(drainDone("w2"))
      .then(decideFinish("w2")).then(finishIdle("w2")).then(finishRegister("w2"))
      .expect(st.suspendCount == 0 and settled and invariant)

  // ---- runs: the mistakes (negative controls) ----------------------------

  run closeDuringRunInvokesAClosedActorTest =
    initWith(CloseDuringRun).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(closeOutside)
      .expect(not(noInvokeAfterClose))

  run keepQueuedRunSubmitsAClosedMailboxTest =
    initWith(KeepQueuedRun).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(closeOutside)
      .expect(not(submittedWithWorkOrStaleRead))

  run panicKeepsTheBitAndStrandsTheMailboxTest =
    initWith(PanicKeepsScheduled).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(take("w1")).then(drainDone("w1")).then(startInvoke("w1")).then(panicInvoke("w1"))
      .expect(not(scheduledMatchesTheRun))

  run panicDropsTheFlagAndHidesASystemMessageTest =
    initWith(PanicDropsSysFlag).then(control("c1", Resume)).then(control("c2", Suspend))
      .then(regRead("c1")).then(regFetchOr("c1")).then(take("w1")).then(clearSysFlag("w1")).then(panicSystem("w1"))
      .expect(not(noHiddenSystemMessage))

  run panicAsRecoverableLosesAResumeTest =
    initWith(PanicAsRecoverable).then(control("c1", Suspend)).then(control("c2", Resume))
      .then(regRead("c1")).then(regFetchOr("c1")).then(regRead("c2")).then(regFetchOr("c2"))
      .then(take("w1")).then(clearSysFlag("w1")).then(processSystem("w1")).then(panicSystem("w1"))
      .then(finishRegister("w1"))
      .expect(settled and st.suspendCount == 1 and not(settledMeansIdle) and not(lossOnlyWhenFatal))

  run silentSystemFailureLeavesTheActorSuspendedTest =
    initWith(SilentSystemFailure).then(control("c1", Suspend)).then(control("c2", Resume))
      .then(regRead("c1")).then(regFetchOr("c1")).then(regRead("c2")).then(regFetchOr("c2"))
      .then(take("w1")).then(clearSysFlag("w1")).then(processSystem("w1")).then(failSystem("w1"))
      .then(drainDone("w1")).then(decideFinish("w1")).then(finishIdle("w1")).then(finishRegister("w1"))
      .expect(settled and st.suspendCount == 1 and not(settledMeansIdle))

  run clearFlagAfterDrainHidesASystemMessageTest =
    initWith(ClearFlagAfterDrain).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(control("c1", Resume)).then(beginDrain("w1")).then(processSystem("w1")).then(drainDone("w1"))
      .then(control("c2", Resume)).then(clearSysFlag("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1"))
      .then(drainDone("w1")).then(startInvoke("w1"))
      .expect(st.sys.length() == 1 and not(noHiddenSystemMessage) and not(systemBeforeNormal))

  run partialDrainLetsAMessageOvertakeTest =
    initWith(PartialDrain).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1")).then(drainDone("w1"))
      .then(control("c1", Resume)).then(control("c2", Suspend)).then(clearSysFlag("w1")).then(processSystem("w1"))
      .then(startInvoke("w1"))
      .expect(st.sys.length() == 1 and not(noSystemSinceDrain))

  run messagesBeforeSystemOvertakesTest =
    initWith(MessagesBeforeSystem).then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(send("s1")).then(register("s1")).then(fetchOr("s1"))
      .then(take("w1")).then(control("c1", Resume)).then(startInvoke("w1")).then(endInvoke("w1"))
      .then(startInvoke("w1"))
      .expect(not(systemBeforeNormal))

  run dropSystemOnCloseLosesAWatchTest =
    initWith(DropSystemOnClose).then(control("c1", Terminate)).then(control("c2", Resume))
      .then(regRead("c1")).then(regFetchOr("c1")).then(take("w1")).then(clearSysFlag("w1")).then(processSystem("w1"))
      .expect(not(systemMessagesConserved))

  run lostWakeupStrandsAMessageTest =
    initWith(LostWakeup).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(endInvoke("w1")).then(decideFinish("w1"))
      .then(send("s2")).then(register("s2")).then(fetchOr("s2")).then(finishIdle("w1"))
      .expect(not(noStrandedMessage))

  run checkThenActRunsTheActorTwiceTest =
    initWith(CheckThenAct).then(send("s1")).then(send("s2")).then(register("s1")).then(register("s2"))
      .then(act("s1")).then(act("s2")).then(take("w1")).then(take("w2"))
      .expect(not(atMostOneRunner) and not(scheduledMatchesTheRun))

  run externalSuspendInterruptsReceiveTest =
    initWith(ExternalSuspend).then(send("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(drainDone("w1")).then(startInvoke("w1")).then(control("c1", Suspend))
      .expect(not(noInvokeWhileSuspended))

  run lateEnqueueLeavesAMessageInAClosedMailboxTest =
    initWith(LateEnqueue).then(send("s1")).then(enqueueLate("s1")).then(register("s1")).then(fetchOr("s1")).then(take("w1"))
      .then(control("c1", Terminate)).then(send("s2")).then(clearSysFlag("w1")).then(processSystem("w1"))
      .then(enqueueLate("s2"))
      .expect(not(closedHoldsNothing))
}
