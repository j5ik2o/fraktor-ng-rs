// TestLatch (crates/fraktor-testkit/src/test_kit/test_latch.rs).
//
// One shared counter, and the moment it reached zero (under the same lock).
// count_down saturates at zero, open sets it to zero, reset sets it back to the
// count the latch was created with and forgets the moment. Every operation may
// happen at any time, also while a ready call waits (from an actor or another
// thread). ready loops: it reads the moment the latch opened, returns when that
// is not after its deadline, fails when the latch opened late or the deadline
// has passed, and otherwise lets time pass. A delivery may run past the
// deadline, so time can pass beyond it before the next check. Time is a step
// counter; DILATED_MAX is the dilated max.
module test_latch {
  pure val DILATED_MAX = 3

  type Op = Created | CountedDown | Opened | ResetOp | Other
  type Waiter = Idle | Waiting | Returned | TimedOut

  var count: int
  var remaining: int
  // The counter before the last operation, and the last operation.
  var before: int
  var lastOp: Op
  var waiter: Waiter
  var now: int
  var deadline: int
  // The step at which the counter reached zero, or -1 while it is closed.
  var openedAt: int
  // The opening moment that the last check of ready read.
  var observed: int

  action init = {
    nondet c = 0.to(3).oneOf()
    all {
      count' = c,
      remaining' = c,
      before' = c,
      lastOp' = Created,
      waiter' = Idle,
      now' = 0,
      deadline' = 0,
      openedAt' = if (c == 0) 0 else -1,
      observed' = if (c == 0) 0 else -1,
    }
  }

  action latchOp(op: Op, value: int): bool = all {
    remaining' = value,
    openedAt' = if (value != 0) -1 else if (openedAt >= 0) openedAt else now,
    before' = remaining,
    lastOp' = op,
    count' = count,
    waiter' = waiter,
    now' = now,
    deadline' = deadline,
    observed' = observed,
  }

  action countDown = latchOp(CountedDown, if (remaining > 0) remaining - 1 else 0)
  action openLatch = latchOp(Opened, 0)
  action reset = latchOp(ResetOp, count)

  action startReady = all {
    waiter == Idle or waiter == Returned or waiter == TimedOut,
    waiter' = Waiting,
    deadline' = now + DILATED_MAX,
    observed' = openedAt,
    openedAt' = openedAt,
    count' = count, remaining' = remaining, before' = before, lastOp' = Other, now' = now,
  }

  // One pass of the loop in ready: read the counter, then decide.
  action readyCheck = all {
    waiter == Waiting,
    observed' = openedAt,
    waiter' = if (openedAt >= 0 and openedAt <= deadline) Returned
      else if (openedAt >= 0 or now >= deadline) TimedOut
      else Waiting,
    openedAt' = openedAt,
    count' = count, remaining' = remaining, before' = before, lastOp' = Other, now' = now,
    deadline' = deadline,
  }

  // Time passes, also beyond a deadline while a delivery runs.
  action tick = all {
    now < 8,
    now' = now + 1,
    count' = count, remaining' = remaining, before' = before, lastOp' = Other,
    waiter' = waiter, deadline' = deadline, observed' = observed, openedAt' = openedAt,
  }

  action step = any { countDown, openLatch, reset, startReady, readyCheck, tick }

  val counterInRange = remaining >= 0 and remaining <= count
  val countDownSaturates = lastOp == CountedDown implies remaining == (if (before > 0) before - 1 else 0)
  val openMeansZero = lastOp == Opened implies remaining == 0
  val resetRestoresCount = lastOp == ResetOp implies remaining == count
  val openedAtMatchesCounter = (remaining == 0) iff (openedAt >= 0 and openedAt <= now)
  val returnedOnlyWhenOpenedInTime = waiter == Returned implies (observed >= 0 and observed <= deadline)
  val timedOutOnlyWhenNotOpenedInTime =
    waiter == TimedOut implies (observed > deadline or (observed < 0 and now >= deadline))
  val invariant = counterInRange and countDownSaturates and openMeansZero and resetRestoresCount
    and openedAtMatchesCounter and returnedOnlyWhenOpenedInTime and timedOutOnlyWhenNotOpenedInTime

  action initWith(c: int): bool = all {
    count' = c, remaining' = c, before' = c, lastOp' = Created, waiter' = Idle,
    now' = 0, deadline' = 0, openedAt' = if (c == 0) 0 else -1, observed' = if (c == 0) 0 else -1,
  }

  run opensAfterCountDownsTest =
    initWith(2).then(startReady).then(countDown).then(readyCheck)
      .then(all { assert(waiter == Waiting), countDown })
      .then(readyCheck)
      .expect(waiter == Returned)

  run timesOutWhileClosedTest =
    initWith(1).then(startReady).then(tick).then(tick).then(tick).then(readyCheck)
      .expect(waiter == TimedOut)

  run resetDuringWaitClosesAgainTest =
    initWith(1).then(startReady).then(countDown).then(reset).then(readyCheck)
      .expect(waiter == Waiting and remaining == 1)

  run openedAfterTheDeadlineTimesOutTest =
    initWith(1).then(startReady).then(tick).then(tick).then(tick).then(tick).then(countDown).then(readyCheck)
      .expect(waiter == TimedOut and remaining == 0)

  run zeroCountIsOpenTest =
    initWith(0).then(startReady).then(readyCheck).expect(waiter == Returned)
}
