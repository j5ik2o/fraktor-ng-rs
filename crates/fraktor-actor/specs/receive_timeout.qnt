// Receive timeout of one actor cell (issue #200).
//
// The model of the design docs/design/issue-200-receive-timeout.md,
// section 7, after Pekko v2.0.0's `dungeon.ReceiveTimeout` and the order
// `ActorCell.invoke` calls it in: before a message that influences the
// timeout is handled, the task is cancelled; after any message, the
// timeout is checked again, rescheduling when the message influences it
// or the handler changed the setting, and scheduling anyway when no task
// is set. A firing task leaves the cell in the fired state and puts one
// `ReceiveTimeout` in the mailbox; once there it is an ordinary message
// and is always delivered while the actor lives (I1), whatever the
// setting has become.
//
// Time is not modelled: `deadlineComes` is the scheduled deadline being
// reached. Durations are not modelled either: a setting is only
// `Undefined`, `Infinite`, or `Finite`. A message other than
// `ReceiveTimeout` is handled in one action; the handled
// `ReceiveTimeout` messages are a separate action, so other messages can
// be handled while one waits in the mailbox. A handler that changes the
// setting is `setTimeout`; whether the message influences the timeout
// does not change the result then, because a changed setting is checked
// with `reschedule = true` either way.
module receive_timeout {
  type Setting = Undefined | Infinite | Finite
  type Task = Empty | Scheduled | Fired

  var setting: Setting
  var task: Task
  // `ReceiveTimeout` messages in the mailbox, delivered, and dropped
  // with the mailbox when the actor stopped.
  var queued: int
  var delivered: int
  var dropped: int
  // Firings, and schedulings (`rescheduleReceiveTimeout` calls). A
  // scheduling is the identity of one deadline.
  var fires: int
  var epoch: int
  var alive: bool
  var restarts: int
  // The last action, and the task and epoch before it, so the I2
  // invariant can compare across one action.
  var lastAction: str
  var prevTask: Task
  var prevEpoch: int

  // `checkReceiveTimeout(reschedule)`: the task after the check, and how
  // many schedulings it made.
  pure def checked(s: Setting, t: Task, reschedule: bool): (Task, int) =
    match s {
      | Finite => if (reschedule or t == Empty) (Scheduled, 1) else (t, 0)
      | Undefined => (Empty, 0)
      | Infinite => (Empty, 0)
    }

  action init = all {
    setting' = Undefined,
    task' = Empty,
    queued' = 0,
    delivered' = 0,
    dropped' = 0,
    fires' = 0,
    epoch' = 0,
    alive' = true,
    restarts' = 0,
    lastAction' = "init",
    prevTask' = Empty,
    prevEpoch' = 0,
  }

  action remember(name: str): bool = all {
    lastAction' = name,
    prevTask' = task,
    prevEpoch' = epoch,
  }

  // A handler sets the timeout to `s`: the version changes, so the check
  // after the message reschedules a finite setting and cancels any other.
  action setTimeout(s: Setting): bool = {
    val after = checked(s, task, true)
    all {
      alive,
      setting' = s,
      task' = after._1,
      epoch' = epoch + after._2,
      queued' = queued, delivered' = delivered, dropped' = dropped,
      fires' = fires, alive' = alive, restarts' = restarts,
      remember("set"),
    }
  }

  // A message that influences the timeout and leaves the setting alone:
  // cancel before, then `checkReceiveTimeout(reschedule = true)`.
  action processInfluencing: bool = {
    val after = checked(setting, Empty, true)
    all {
      alive,
      task' = after._1,
      epoch' = epoch + after._2,
      setting' = setting,
      queued' = queued, delivered' = delivered, dropped' = dropped,
      fires' = fires, alive' = alive, restarts' = restarts,
      remember("influencing"),
    }
  }

  // A `NotInfluenceReceiveTimeout` message that leaves the setting alone:
  // nothing is cancelled, and the check schedules only when no task is set.
  action processNotInfluencing: bool = {
    val after = checked(setting, task, false)
    all {
      alive,
      task' = after._1,
      epoch' = epoch + after._2,
      setting' = setting,
      queued' = queued, delivered' = delivered, dropped' = dropped,
      fires' = fires, alive' = alive, restarts' = restarts,
      remember("notInfluencing"),
    }
  }

  // The scheduled deadline is reached: the task fires once, and its
  // `ReceiveTimeout` goes into the mailbox.
  action deadlineComes: bool = all {
    alive,
    task == Scheduled,
    task' = Fired,
    queued' = queued + 1,
    fires' = fires + 1,
    setting' = setting, epoch' = epoch,
    delivered' = delivered, dropped' = dropped, alive' = alive, restarts' = restarts,
    remember("fire"),
  }

  // A `ReceiveTimeout` in the mailbox is handled: it is delivered as an
  // ordinary message that influences the timeout.
  action processReceiveTimeout: bool = {
    val after = checked(setting, Empty, true)
    all {
      alive,
      queued > 0,
      queued' = queued - 1,
      delivered' = delivered + 1,
      task' = after._1,
      epoch' = epoch + after._2,
      setting' = setting, dropped' = dropped, fires' = fires, alive' = alive, restarts' = restarts,
      remember("receiveTimeout"),
    }
  }

  // `terminate`: `cancelReceiveTimeout()` and `cancelReceiveTimeoutTask()`.
  // The mailbox, with any `ReceiveTimeout` in it, goes to dead letters.
  action stop: bool = all {
    alive,
    alive' = false,
    setting' = Undefined,
    task' = Empty,
    queued' = 0,
    dropped' = dropped + queued,
    delivered' = delivered, fires' = fires, epoch' = epoch, restarts' = restarts,
    remember("stop"),
  }

  // A failing message that influences the timeout, then the restart:
  // the failed message is checked as usual, and `finishRecreate` runs
  // `checkReceiveTimeout(reschedule = true)`. The setting survives the
  // restart, and so does the mailbox.
  action restart: bool = {
    val failed = checked(setting, Empty, true)
    val recreated = checked(setting, failed._1, true)
    all {
      alive,
      task' = recreated._1,
      epoch' = epoch + failed._2 + recreated._2,
      restarts' = restarts + 1,
      setting' = setting,
      queued' = queued, delivered' = delivered, dropped' = dropped, fires' = fires, alive' = alive,
      remember("restart"),
    }
  }

  // Negative control for I1: the `ReceiveTimeout` is dropped on receipt
  // when the setting is no longer finite.
  action processReceiveTimeoutBugDiscards: bool = {
    val after = checked(setting, Empty, true)
    all {
      alive,
      queued > 0,
      queued' = queued - 1,
      delivered' = if (setting == Finite) delivered + 1 else delivered,
      task' = after._1,
      epoch' = epoch + after._2,
      setting' = setting, dropped' = dropped, fires' = fires, alive' = alive, restarts' = restarts,
      remember("receiveTimeout"),
    }
  }

  // Negative control for I4: firing does not clear the deadline.
  action deadlineComesBugKeeps: bool = all {
    alive,
    task == Scheduled,
    task' = Scheduled,
    queued' = queued + 1,
    fires' = fires + 1,
    setting' = setting, epoch' = epoch,
    delivered' = delivered, dropped' = dropped, alive' = alive, restarts' = restarts,
    remember("fire"),
  }

  // Negative control for I2: a message that does not influence the
  // timeout cancels and reschedules like one that does.
  action processNotInfluencingBugCancels: bool = {
    val after = checked(setting, Empty, true)
    all {
      alive,
      task' = after._1,
      epoch' = epoch + after._2,
      setting' = setting,
      queued' = queued, delivered' = delivered, dropped' = dropped,
      fires' = fires, alive' = alive, restarts' = restarts,
      remember("notInfluencing"),
    }
  }

  action step = any {
    nondet s = Set(Undefined, Infinite, Finite).oneOf()
    setTimeout(s),
    processInfluencing,
    processNotInfluencing,
    deadlineComes,
    processReceiveTimeout,
    stop,
    restart,
  }

  // I1: a deadline exists only under a finite setting, so a firing
  // enqueues a `ReceiveTimeout` only then; every enqueued one is
  // delivered while the actor lives, and is lost only with the mailbox of
  // a stopped actor.
  val scheduledOnlyWhenFinite = task == Scheduled implies setting == Finite
  val firedAreDeliveredOrQueued = delivered + queued + dropped == fires
  val droppedOnlyWhenStopped = alive or queued == 0
  val liveActorDropsNothing = alive implies dropped == 0
  // I4: one firing per scheduling, so one `ReceiveTimeout` per firing.
  val oneMessagePerFiring = fires <= epoch
  // I2: a message that does not influence the timeout and leaves the
  // setting alone does not move a deadline and does not rearm a fired
  // task.
  val notInfluencingKeepsDeadline =
    (lastAction == "notInfluencing" and prevTask != Empty) implies (task == prevTask and epoch == prevEpoch)
  // A stopped actor has no deadline.
  val stoppedHasNoDeadline = alive or task == Empty

  val invariant = scheduledOnlyWhenFinite
    and firedAreDeliveredOrQueued
    and droppedOnlyWhenStopped
    and liveActorDropsNothing
    and oneMessagePerFiring
    and notInfluencingKeepsDeadline
    and stoppedHasNoDeadline

  // Pekko delivers a `ReceiveTimeout` that was enqueued before the timeout
  // was turned off.
  run firedIsDeliveredAfterTurnOffTest =
    init
      .then(setTimeout(Finite))
      .then(deadlineComes)
      .then(setTimeout(Undefined))
      .expect(task == Empty and queued == 1)
      .then(processReceiveTimeout)
      .expect(delivered == 1 and task == Empty and invariant)

  // A message that does not influence the timeout does not rearm a fired
  // task, and the `ReceiveTimeout` then rearms it.
  run notInfluencingDoesNotRearmFiredTest =
    init
      .then(setTimeout(Finite))
      .then(deadlineComes)
      .then(processNotInfluencing)
      .expect(task == Fired and invariant)
      .then(processReceiveTimeout)
      .expect(task == Scheduled and delivered == 1)

  // A setting changed while a `ReceiveTimeout` waits schedules a new
  // deadline; two `ReceiveTimeout` messages can then be delivered, as in
  // Pekko.
  run twoDeliveriesAreAllowedTest =
    init
      .then(setTimeout(Finite))
      .then(deadlineComes)
      .then(setTimeout(Finite))
      .then(deadlineComes)
      .expect(queued == 2 and invariant)
      .then(processReceiveTimeout)
      .then(processReceiveTimeout)
      .expect(delivered == 2 and invariant)

  // The setting survives a restart, and the timeout is scheduled again.
  run restartKeepsTheSettingTest =
    init
      .then(setTimeout(Finite))
      .then(deadlineComes)
      .then(restart)
      .expect(setting == Finite and task == Scheduled and queued == 1 and invariant)

  // A stopped actor never fires.
  run stopCancelsTheTimeoutTest =
    init
      .then(setTimeout(Finite))
      .then(stop)
      .expect(task == Empty and invariant)
      .then(deadlineComes)
      .fail()

  // Negative control for I1.
  run bugDiscardsOnReceiptTest =
    init
      .then(setTimeout(Finite))
      .then(deadlineComes)
      .then(setTimeout(Undefined))
      .then(processReceiveTimeoutBugDiscards)
      .expect(not(firedAreDeliveredOrQueued))

  // Negative control for I4.
  run bugKeepsTheDeadlineOnFireTest =
    init
      .then(setTimeout(Finite))
      .then(deadlineComesBugKeeps)
      .then(deadlineComesBugKeeps)
      .expect(not(oneMessagePerFiring))

  // Negative control for I2.
  run bugNotInfluencingCancelsTest =
    init
      .then(setTimeout(Finite))
      .then(processNotInfluencingBugCancels)
      .expect(not(notInfluencingKeepsDeadline))
}
