// token_bucket (crates/fraktor-actor/src/kernel/token_bucket/token_bucket_value.rs).
//
// The bucket holds up to Capacity tokens and gains one token every Period
// nanoseconds. `init` fills it and anchors the arrival grid at the init
// time `t0`: tokens arrive only at `t0 + k * Period`. An offer that the
// available tokens cannot cover returns a delay and moves `lastUpdate` to
// the grid point at which the last needed token arrives, which is
// `remaining` grid points ahead, not the next one. The delay subtracts the
// time since the last arrival, so the grid never shifts.
//
// The model keeps the caller's contract (design T8): an offer is only made
// when `now >= lastUpdate`, that is, not before the delay of a previous
// offer has elapsed. Time is an unbounded `int`, so the `i64` wrap-around
// of the code is not modeled; the Rust tests cover it.
module token_bucket {
  pure val Capacity = 5
  pure val Period = 3
  pure val StartTimes = Set(0, -30, 7)

  var available: int
  var lastUpdate: int
  var now: int
  var t0: int
  var started: bool
  // Ghosts of the last offer, for the invariants that speak about it.
  var lastDelay: int
  var lastRemaining: int
  var lastElapsed: int
  var lastArrived: int

  action initAt(t: int): bool = all {
    available' = Capacity,
    lastUpdate' = t,
    now' = t,
    t0' = t,
    started' = true,
    lastDelay' = 0,
    lastRemaining' = 0,
    lastElapsed' = 0,
    lastArrived' = 0,
  }

  action init = {
    nondet t = StartTimes.oneOf()
    initAt(t)
  }

  def elapsed: int = now - lastUpdate
  def arrived: int = if (elapsed >= Period) elapsed / Period else 0
  def afterArrival(bug: str): int =
    if (bug == "nocap") available + arrived
    else if (available + arrived > Capacity) Capacity
    else available + arrived
  def gridBase: int = lastUpdate + arrived * Period

  // bug is "none" for the bucket itself, or the name of a defect for the
  // negative controls: "naive" (the delay does not subtract the time since
  // the last arrival), "extra" (the delay is one period too long), "nocap"
  // (arrivals are not capped at the capacity).
  action offerWith(bug: str, cost: int): bool = all {
    started,
    now >= lastUpdate,
    available' = if (cost <= afterArrival(bug)) afterArrival(bug) - cost else 0,
    lastUpdate' =
      if (cost <= afterArrival(bug)) gridBase
      else now + (
        if (bug == "naive") (cost - afterArrival(bug)) * Period
        else if (bug == "extra") (cost - afterArrival(bug)) * Period - (now - gridBase) + Period
        else (cost - afterArrival(bug)) * Period - (now - gridBase)
      ),
    now' = now,
    t0' = t0,
    started' = started,
    lastDelay' =
      if (cost <= afterArrival(bug)) 0
      else if (bug == "naive") (cost - afterArrival(bug)) * Period
      else if (bug == "extra") (cost - afterArrival(bug)) * Period - (now - gridBase) + Period
      else (cost - afterArrival(bug)) * Period - (now - gridBase),
    lastRemaining' = if (cost <= afterArrival(bug)) 0 else cost - afterArrival(bug),
    lastElapsed' = elapsed,
    lastArrived' = arrived,
  }

  action advanceBy(d: int): bool = all {
    started,
    d >= 0,
    available' = available,
    lastUpdate' = lastUpdate,
    now' = now + d,
    t0' = t0,
    started' = started,
    lastDelay' = lastDelay,
    lastRemaining' = lastRemaining,
    lastElapsed' = lastElapsed,
    lastArrived' = lastArrived,
  }

  action advanceStep = {
    nondet d = 0.to(20).oneOf()
    advanceBy(d)
  }

  action offerStep = {
    nondet cost = 0.to(12).oneOf()
    offerWith("none", cost)
  }

  action step = any { init, advanceStep, offerStep }

  val availableWithinCapacity = available >= 0 and available <= Capacity
  val lastUpdateOnGrid = (lastUpdate - t0) % Period == 0
  val delayNonNegative = lastDelay >= 0
  val delayIsEarliest =
    lastDelay == 0 or ((lastRemaining - 1) * Period < lastDelay and lastDelay <= lastRemaining * Period)
  val arrivalCountMatchesElapsed =
    lastArrived == (if (lastElapsed >= Period) lastElapsed / Period else 0)
  val invariant =
    availableWithinCapacity and lastUpdateOnGrid and delayNonNegative and delayIsEarliest
      and arrivalCountMatchesElapsed

  run startsFullTest =
    initAt(0).then(offerWith("none", 5)).expect(available == 0 and lastDelay == 0)

  run renewalGridTest =
    initAt(0)
      .then(offerWith("none", 10))
      .expect(lastDelay == 15 and available == 0)
      .then(advanceBy(16))
      .then(offerWith("none", 1))
      .expect(lastDelay == 2)

  run bugNaiveDelayTest =
    initAt(0)
      .then(offerWith("none", 10))
      .then(advanceBy(16))
      .then(offerWith("naive", 1))
      .expect(not(lastUpdateOnGrid))

  run bugExtraPeriodTest =
    initAt(0)
      .then(offerWith("none", 10))
      .then(advanceBy(16))
      .then(offerWith("extra", 1))
      .expect(lastDelay == 5 and not(delayIsEarliest))

  run bugNoCapacityCapTest =
    initAt(0).then(advanceBy(100)).then(offerWith("nocap", 0)).expect(not(availableWithinCapacity))
}
