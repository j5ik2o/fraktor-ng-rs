// bounded_stash (crates/fraktor-actor/src/kernel/actor, the bounded
// stash of #192: ActorContext::try_stash / unstash_all and the
// executor's dead letter paths).
//
// Messages carry consecutive ids in arrival order. `queued` is the
// mailbox content for the one actor (oldest first), `stashed` the
// stash (oldest first), `dead` the dead letters in publication order,
// and `handled` the messages the actor processed. The capacities are
// one fixed instance (stash 3, mailbox 3); the unlimited case is the
// absence of a bound and is covered by the Rust tests, and capacity 0
// has no representation in the code (design M1), so it is not modeled.
//
// `victimOlderThan` is a ghost set of (victim, survivor) pairs recorded
// at every overflow: the invariant that every victim is older than
// every survivor of its overflow is what "the oldest messages are
// dropped" means once later arrivals have larger ids.
module bounded_stash {
  pure val StashCap = 3
  pure val MailboxCap = 3

  var stashed: List[int]
  var queued: List[int]
  var dead: List[int]
  var handled: List[int]
  var nextId: int
  var stopped: bool
  var victimOlderThan: Set[(int, int)]

  action init = all {
    stashed' = [],
    queued' = [],
    dead' = [],
    handled' = [],
    nextId' = 0,
    stopped' = false,
    victimOlderThan' = Set(),
  }

  // A normal delivery that lands in the bounded mailbox. The bound is
  // what lets the model build a queued backlog for `unstashAll`.
  action tell = all {
    not(stopped),
    queued.length() < MailboxCap,
    queued' = queued.append(nextId),
    nextId' = nextId + 1,
    stashed' = stashed,
    dead' = dead,
    handled' = handled,
    stopped' = stopped,
    victimOlderThan' = victimOlderThan,
  }

  // The actor stashes the oldest queued message.
  action stashNext = all {
    not(stopped),
    queued.length() > 0,
    stashed.length() < StashCap,
    stashed' = stashed.append(queued.head()),
    queued' = queued.tail(),
    dead' = dead,
    handled' = handled,
    nextId' = nextId,
    stopped' = stopped,
    victimOlderThan' = victimOlderThan,
  }

  // The off-by-one defect: one message too many fits the stash.
  action stashNextBugOffByOne = all {
    not(stopped),
    queued.length() > 0,
    stashed.length() <= StashCap,
    stashed' = stashed.append(queued.head()),
    queued' = queued.tail(),
    dead' = dead,
    handled' = handled,
    nextId' = nextId,
    stopped' = stopped,
    victimOlderThan' = victimOlderThan,
  }

  // unstashAll with a choice of which side to drop: "oldest" is the
  // code, "newest" is the negative control. The newest `room` stashed
  // messages are prepended to the mailbox; the rest go to dead
  // letters, newest victim first.
  action unstashAllWith(drop: str): bool = all {
    not(stopped),
    val room = MailboxCap - queued.length()
    val excess = if (stashed.length() > room) stashed.length() - room else 0
    val victims = if (drop == "newest") stashed.slice(stashed.length() - excess, stashed.length()) else stashed.slice(0, excess)
    val keep = if (drop == "newest") stashed.slice(0, stashed.length() - excess) else stashed.slice(excess, stashed.length())
    all {
      dead' = dead.concat(reversed(victims)),
      queued' = keep.concat(queued),
      stashed' = [],
      victimOlderThan' = victims.foldl(victimOlderThan, (pairs, v) => pairs.union(keep.foldl(Set(), (s, x) => s.union(Set((v, x)))))),
      handled' = handled,
      nextId' = nextId,
      stopped' = stopped,
    }
  }

  action unstashAll = unstashAllWith("oldest")
  action unstashAllBugNewest = unstashAllWith("newest")

  action handleOne = all {
    not(stopped),
    queued.length() > 0,
    handled' = handled.append(queued.head()),
    queued' = queued.tail(),
    stashed' = stashed,
    dead' = dead,
    nextId' = nextId,
    stopped' = stopped,
    victimOlderThan' = victimOlderThan,
  }

  // The stop: postStop returns the stash to the bounded mailbox (the
  // same overflow choice as unstashAll), then the mailbox clean-up
  // hands everything left, in order, to dead letters.
  action stopWith(drop: str): bool = all {
    not(stopped),
    val room = if (MailboxCap > queued.length()) MailboxCap - queued.length() else 0
    val excess = if (stashed.length() > room) stashed.length() - room else 0
    val victims = if (drop == "newest") stashed.slice(stashed.length() - excess, stashed.length()) else stashed.slice(0, excess)
    val keep = if (drop == "newest") stashed.slice(0, stashed.length() - excess) else stashed.slice(excess, stashed.length())
    all {
      dead' = dead.concat(reversed(victims)).concat(keep).concat(queued),
      queued' = [],
      stashed' = [],
      stopped' = true,
      victimOlderThan' = victims.foldl(victimOlderThan, (pairs, v) => pairs.union(keep.foldl(Set(), (s, x) => s.union(Set((v, x)))))),
      handled' = handled,
      nextId' = nextId,
    }
  }

  action stop = stopWith("oldest")

  // The defect that drops the stash on stop instead of returning it.
  action stopBugDropsStash = all {
    not(stopped),
    dead' = dead.concat(queued),
    queued' = [],
    stashed' = [],
    stopped' = true,
    victimOlderThan' = victimOlderThan,
    handled' = handled,
    nextId' = nextId,
  }

  action step = any {
    init, tell, stashNext, unstashAll, handleOne, stop,
  }

  pure def reversed(l: List[int]): List[int] = l.foldl([], (acc, x) => [x].concat(acc))

  def allIds: List[int] = stashed.concat(queued).concat(dead).concat(handled)
  def idSet: Set[int] = allIds.foldl(Set(), (s, x) => s.union(Set(x)))

  val stashWithinCapacity = stashed.length() <= StashCap
  val noLossNoDuplication = allIds.length() == nextId and idSet.size() == nextId
  val deadLettersAreOldestOnOverflow = victimOlderThan.forall(p => p._1 < p._2)
  val invariant = stashWithinCapacity and noLossNoDuplication and deadLettersAreOldestOnOverflow

  run overflowDropsTheOldestTest =
    init
      .then(tell).then(tell).then(tell)
      .then(stashNext).then(stashNext).then(stashNext)
      .then(tell).then(tell)
      .then(unstashAll)
      .expect(dead == [1, 0] and queued == [2, 3, 4] and stashed == [])

  run stopReturnsTheStashInOrderTest =
    init
      .then(tell).then(tell)
      .then(stashNext).then(stashNext)
      .then(stop)
      .expect(dead == [0, 1] and stopped)

  run bugNewestDroppedTest =
    init
      .then(tell).then(tell).then(tell)
      .then(stashNext).then(stashNext).then(stashNext)
      .then(tell).then(tell)
      .then(unstashAllBugNewest)
      .expect(not(deadLettersAreOldestOnOverflow))

  run bugOffByOneCapacityTest =
    init
      .then(tell).then(tell).then(tell)
      .then(stashNext).then(stashNext).then(stashNext)
      .then(tell)
      .then(stashNextBugOffByOne)
      .expect(not(stashWithinCapacity))

  run bugStopDropsStashTest =
    init
      .then(tell).then(tell)
      .then(stashNext).then(stashNext)
      .then(stopBugDropsStash)
      .expect(not(noLossNoDuplication))
}
