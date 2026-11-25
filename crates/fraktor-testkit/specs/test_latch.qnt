// TestLatch (crates/fraktor-testkit/src/test_kit/test_latch.rs).
//
// The latch keeps one shared counter. count_down saturates at zero, open sets
// it to zero, reset sets it back to the original count, and ready returns only
// while the counter is zero (it panics at its deadline otherwise).
module test_latch {
  pure val COUNT = 3

  var remaining: int
  // Whether a ready call has returned (as opposed to timing out).
  var readyReturned: bool
  // The counter value seen by the last ready that returned.
  var seenByReady: int

  action init = all {
    remaining' = COUNT,
    readyReturned' = false,
    seenByReady' = COUNT,
  }

  action countDown = all {
    remaining' = if (remaining > 0) remaining - 1 else 0,
    readyReturned' = readyReturned,
    seenByReady' = seenByReady,
  }

  action openLatch = all {
    remaining' = 0,
    readyReturned' = readyReturned,
    seenByReady' = seenByReady,
  }

  action reset = all {
    remaining' = COUNT,
    readyReturned' = readyReturned,
    seenByReady' = seenByReady,
  }

  // ready checks the counter before every delivery and returns once it is 0.
  action readyReturns = all {
    remaining == 0,
    remaining' = remaining,
    readyReturned' = true,
    seenByReady' = remaining,
  }

  action step = any { countDown, openLatch, reset, readyReturns }

  val counterInRange = remaining >= 0 and remaining <= COUNT
  val readyOnlyWhenOpen = readyReturned implies seenByReady == 0
  val invariant = counterInRange and readyOnlyWhenOpen

  run opensAfterCountDownsTest =
    init.then(countDown).then(countDown).then(countDown).then(readyReturns)
      .then(all { assert(remaining == 0), countDown })
      .then(all { assert(remaining == 0), reset })
      .then(all { assert(remaining == COUNT), openLatch })
      .then(all { assert(remaining == 0), readyReturns })

  run readyBlockedWhileClosedTest =
    init.then(countDown).then(readyReturns).fail()
}
