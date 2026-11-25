// receive_while (crates/fraktor-testkit/src/test_kit/test_kit_base.rs).
//
// Messages are integers; `pick` accepts the even ones. The call takes the head
// of the queue while it is accepted and fewer than LIMIT have been picked. A
// rejected head goes back to the front of the queue. A wait that times out
// (no message, idle or max passed) ends the call. New messages may arrive at
// the tail at any time. LIMIT -1 is "no limit"; with LIMIT 0 the call ends
// before its first wait and keeps the last message it found (BEFORE).
module receive_while {
  pure val MESSAGES = Set(1, 2, 3, 4)

  var queue: List[int]
  var picked: List[int]
  var done: bool
  // Every message that ever arrived, in arrival order.
  var arrived: List[int]
  // The last message the kit reports as taken (-1 is "none").
  var last: int
  // The last message before the call, and the limit of the call.
  var lastBefore: int
  var limit: int

  pure def accepts(m: int): bool = m % 2 == 0
  def underLimit: bool = limit < 0 or picked.length() < limit

  action init = {
    nondet first = MESSAGES.oneOf()
    nondet second = MESSAGES.oneOf()
    nondet previous = Set(-1).union(MESSAGES).oneOf()
    nondet l = (-1).to(3).oneOf()
    all {
      queue' = [first, second],
      arrived' = [first, second],
      picked' = [],
      done' = false,
      last' = previous,
      lastBefore' = previous,
      limit' = l,
    }
  }

  action arrive = {
    nondet m = MESSAGES.oneOf()
    all {
      arrived.length() < 5,
      queue' = queue.append(m),
      arrived' = arrived.append(m),
      picked' = picked,
      done' = done,
      last' = last,
      lastBefore' = lastBefore,
      limit' = limit,
    }
  }

  action takeAccepted = all {
    not(done),
    underLimit,
    queue.length() > 0,
    accepts(queue.head()),
    picked' = picked.append(queue.head()),
    last' = queue.head(),
    queue' = queue.tail(),
    done' = done,
    arrived' = arrived,
    lastBefore' = lastBefore,
    limit' = limit,
  }

  // The loop condition fails: the limit is reached (at once when it is 0).
  action limitReached = all {
    not(done),
    not(underLimit),
    done' = true,
    queue' = queue,
    picked' = picked,
    last' = last,
    arrived' = arrived,
    lastBefore' = lastBefore,
    limit' = limit,
  }

  // The rejected head is taken and put back at the front: the queue is unchanged.
  action putBackRejected = all {
    not(done),
    underLimit,
    queue.length() > 0,
    not(accepts(queue.head())),
    queue' = queue,
    done' = true,
    picked' = picked,
    last' = if (picked.length() == 0) -1 else last,
    arrived' = arrived,
    lastBefore' = lastBefore,
    limit' = limit,
  }

  action timeOut = all {
    not(done),
    underLimit,
    done' = true,
    queue' = queue,
    picked' = picked,
    last' = if (picked.length() == 0) -1 else last,
    arrived' = arrived,
    lastBefore' = lastBefore,
    limit' = limit,
  }

  action step = any { arrive, takeAccepted, limitReached, putBackRejected, timeOut }

  // Nothing is lost or reordered: what was picked, followed by what is still
  // queued, is exactly what arrived.
  val nothingLostOrReordered = picked.concat(queue) == arrived
  val pickedOnlyAccepted = picked.indices().forall(i => accepts(picked[i]))
  val withinLimit = limit < 0 or picked.length() <= limit
  // While running and after a wait, `last` is the last picked message, or none.
  // A limit of 0 ends the call before any wait, so `last` stays as before.
  val lastIsLastPicked =
    if (picked.length() > 0) last == picked[picked.length() - 1]
    else if (limit == 0 or not(done)) last == lastBefore
    else last == -1
  val invariant = nothingLostOrReordered and pickedOnlyAccepted and withinLimit and lastIsLastPicked

  action initWith(messages: List[int], l: int): bool = all {
    queue' = messages,
    arrived' = messages,
    picked' = [],
    done' = false,
    last' = 9,
    lastBefore' = 9,
    limit' = l,
  }

  run rejectedStaysAtHeadTest =
    initWith([2, 3], -1).then(takeAccepted).then(putBackRejected)
      .expect(picked == [2] and queue == [3] and done and last == 2)

  run limitEndsTheCallTest =
    initWith([2, 4, 6], 2).then(takeAccepted).then(takeAccepted).then(limitReached)
      .expect(picked == [2, 4] and queue == [6] and done and last == 4)

  run timeOutKeepsTheQueueTest =
    initWith([], -1).then(timeOut).expect(picked == [] and queue == [] and done and last == -1)

  run zeroLimitKeepsTheLastMessageTest =
    initWith([2], 0).then(limitReached).expect(picked == [] and queue == [2] and done and last == 9)
}
