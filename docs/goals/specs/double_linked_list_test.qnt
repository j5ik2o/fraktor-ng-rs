// Instances and tests of the model in double_linked_list.qnt.
//
// `fixed` is the code at PR #178 HEAD 6c267a1: a handle keeps the number of the
// list that made it. `buggy` is the code before that fix, where a handle was
// looked up by its node number alone; it is the negative control and must
// violate `handlesNameExactlyTheGhostNodes`.
module fixed {
  import double_linked_list(IgnoreList = false).* from "./double_linked_list"

  // A handle of list 1 with the same node number as a node of list 0 does not
  // name that node.
  run foreignHandleWithTheSameNumberIsNotFound =
    init
      .then(appendValue(0, 1))
      .then(appendValue(1, 2))
      .then(all {
        assert(issued.contains({ list: 1, n: 0 })),
        assert(not(found(lists.get(0), { list: 1, n: 0 }))),
        assert(inv),
        lists' = lists, ghost' = ghost, issued' = issued, issueCount' = issueCount, resultOk' = resultOk,
      })

  // A removed handle names nothing, and the next node gets a new number.
  run removedHandleIsNotFoundAndNotReused =
    init
      .then(appendValue(0, 1))
      .then(removeNode(0, { list: 0, n: 0 }))
      .then(appendValue(0, 1))
      .then(all {
        assert(not(found(lists.get(0), { list: 0, n: 0 }))),
        assert(keysOf(ghost.get(0)) == List({ list: 0, n: 1 })),
        assert(inv),
        lists' = lists, ghost' = ghost, issued' = issued, issueCount' = issueCount, resultOk' = resultOk,
      })

  // The original's "move elements to back": c, b, a, then move a, b, c.
  run moveElementsToBack =
    init
      .then(appendValue(0, 2))
      .then(appendValue(0, 1))
      .then(appendValue(0, 0))
      .then(moveToBack(0, { list: 0, n: 2 }))
      .then(moveToBack(0, { list: 0, n: 1 }))
      .then(moveToBack(0, { list: 0, n: 0 }))
      .then(all {
        assert(keysOf(ghost.get(0)) == List({ list: 0, n: 2 }, { list: 0, n: 1 }, { list: 0, n: 0 })),
        assert(inv),
        lists' = lists, ghost' = ghost, issued' = issued, issueCount' = issueCount, resultOk' = resultOk,
      })

  // The original's "find next element or else insert new element", with values
  // a = 0, b = 1, c = 2: from a1, walk past the a's (v <= 0) and find a c.
  run findNextElementOrElseInsert =
    init
      .then(prependValue(0, 2))
      .then(prependValue(0, 2))
      .then(prependValue(0, 0))
      .then(prependValue(0, 0))
      .then(findOrElseInsert(0, Forward, { list: 0, n: 3 }, 0, 2, 1))
      .then(all {
        assert(issueCount == 4),
        assert(resultOk),
        lists' = lists, ghost' = ghost, issued' = issued, issueCount' = issueCount, resultOk' = resultOk,
      })
      .then(findOrElseInsert(0, Forward, { list: 0, n: 3 }, 0, 1, 1))
      .then(all {
        assert(issueCount == 5),
        assert(keysOf(ghost.get(0)) == List({ list: 0, n: 3 }, { list: 0, n: 2 }, { list: 0, n: 4 }, { list: 0, n: 1 }, { list: 0, n: 0 })),
        assert(valuesOf(ghost.get(0)) == List(0, 0, 1, 2, 2)),
        assert(inv),
        lists' = lists, ghost' = ghost, issued' = issued, issueCount' = issueCount, resultOk' = resultOk,
      })
}

module buggy {
  import double_linked_list(IgnoreList = true).* from "./double_linked_list"
}
