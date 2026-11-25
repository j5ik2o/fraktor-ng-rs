// receive_while (crates/fraktor-testkit/src/test_kit/test_kit_base.rs).
//
// Messages are integers; `pick` accepts the even ones. The call takes the head
// of the queue while it is accepted and fewer than LIMIT have been picked. A
// rejected head goes back to the front of the queue. A wait that times out
// (no message, idle or max passed) ends the call. New messages may arrive at
// the tail at any time.
module receive_while {
  pure val LIMIT = 2
  pure val MESSAGES = Set(1, 2, 3, 4)

  var queue: List[int]
  var picked: List[int]
  var done: bool
  // Every message that ever arrived, in arrival order.
  var arrived: List[int]
  // The last message the kit reports as taken (-1 is "none").
  var last: int

  pure def accepts(m: int): bool = m % 2 == 0

  action init = {
    nondet first = MESSAGES.oneOf()
    nondet second = MESSAGES.oneOf()
    all {
      queue' = [first, second],
      arrived' = [first, second],
      picked' = [],
      done' = false,
      last' = -1,
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
    }
  }

  action takeAccepted = all {
    not(done),
    picked.length() < LIMIT,
    queue.length() > 0,
    accepts(queue.head()),
    picked' = picked.append(queue.head()),
    last' = queue.head(),
    queue' = queue.tail(),
    done' = picked.length() + 1 >= LIMIT,
    arrived' = arrived,
  }

  // The rejected head is taken and put back at the front: the queue is unchanged.
  action putBackRejected = all {
    not(done),
    queue.length() > 0,
    not(accepts(queue.head())),
    queue' = queue,
    done' = true,
    picked' = picked,
    last' = last,
    arrived' = arrived,
  }

  action timeOut = all {
    not(done),
    done' = true,
    queue' = queue,
    picked' = picked,
    last' = last,
    arrived' = arrived,
  }

  action step = any { arrive, takeAccepted, putBackRejected, timeOut }

  // Nothing is lost or reordered: what was picked, followed by what is still
  // queued, is exactly what arrived.
  val nothingLostOrReordered = picked.concat(queue) == arrived
  val pickedOnlyAccepted = picked.indices().forall(i => accepts(picked[i]))
  val withinLimit = picked.length() <= LIMIT
  val lastIsLastPicked =
    if (picked.length() == 0) last == -1 else last == picked[picked.length() - 1]
  val invariant = nothingLostOrReordered and pickedOnlyAccepted and withinLimit and lastIsLastPicked

  action initWith(messages: List[int]): bool = all {
    queue' = messages,
    arrived' = messages,
    picked' = [],
    done' = false,
    last' = -1,
  }

  run rejectedStaysAtHeadTest =
    initWith([2, 3]).then(takeAccepted).then(putBackRejected)
      .expect(picked == [2] and queue == [3] and done and last == 2)

  run limitEndsTheCallTest =
    initWith([2, 4, 6]).then(takeAccepted).then(takeAccepted)
      .expect(picked == [2, 4] and queue == [6] and done)

  run timeOutKeepsTheQueueTest =
    initWith([]).then(timeOut).expect(picked == [] and queue == [] and done and last == -1)
}
