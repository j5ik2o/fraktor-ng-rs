// A model of crates/fraktor-actor/src/kernel/double_linked_list (PR #178, HEAD 6c267a1).
//
// Two lists live side by side so that a handle of one list can be used on the
// other. Each list keeps its nodes in a map keyed by handle and links them with
// `prev` and `next`, as `DoubleLinkedList` does with its `BTreeMap<NodeId, Node>`.
// Every mutating operation is written twice: once on the links, step by step as
// the Rust code does (`insert_before`, `insert_after`, `link`, `unlink`,
// `shift_while`), and once on a ghost sequence that states what Pekko's
// `DoubleLinkedList` does to the order. The invariants say that the links stay
// consistent, that walking them gives the ghost sequence, that each operation
// returns the node the ghost semantics names, and that a handle names a node of
// a list exactly when the ghost sequence of that list holds it.
//
// `IgnoreList` models the code before the fix of the Devin finding on PR #178:
// a handle was looked up by its node number alone, so the number of the list
// that made it was ignored. With `IgnoreList = true`, `handlesNameExactlyTheGhostNodes`
// is violated (the negative control in double_linked_list_test.qnt).
module double_linked_list {
  const IgnoreList: bool

  pure val Lists: Set[int] = Set(0, 1)
  pure val Values: Set[int] = Set(0, 1, 2)
  pure val MaxLen: int = 5

  type Handle = { list: int, n: int }
  type Link = Nil | To(Handle)
  type Entry = { prev: Link, next: Link, value: int }
  type ListState = { nodes: Handle -> Entry, first: Link, last: Link, nextId: int }
  type Dir = Forward | Backward
  type Made = { s: ListState, h: Handle }
  type GEntry = { h: Handle, v: int }

  // The lists, keyed by list number.
  var lists: int -> ListState
  // The ghost order of each list: the handles and values from the first to the
  // last node.
  var ghost: int -> List[GEntry]
  // Every handle any operation has returned for a new node, and how many.
  var issued: Set[Handle]
  var issueCount: int
  // Whether the last operation returned the node the ghost semantics names.
  var resultOk: bool

  // --- the lookup key -------------------------------------------------------

  // The key a handle is looked up by. The fix keeps the list number in it.
  pure def keyOf(h: Handle): Handle = if (IgnoreList) { list: 0, n: h.n } else h

  pure def found(s: ListState, h: Handle): bool = s.nodes.keys().contains(keyOf(h))

  // --- the links, as the Rust code changes them ------------------------------

  pure def mapRemove(m: Handle -> Entry, k: Handle): Handle -> Entry =
    m.keys().exclude(Set(k)).mapBy(x => m.get(x))

  pure def setPrev(s: ListState, k: Handle, l: Link): ListState =
    { ...s, nodes: s.nodes.put(k, { ...s.nodes.get(k), prev: l }) }

  pure def setNext(s: ListState, k: Handle, l: Link): ListState =
    { ...s, nodes: s.nodes.put(k, { ...s.nodes.get(k), next: l }) }

  pure def prevOf(s: ListState, l: Link): Link = match l {
    | Nil => Nil
    | To(k) => s.nodes.get(k).prev
  }

  pure def nextOf(s: ListState, l: Link): Link = match l {
    | Nil => Nil
    | To(k) => s.nodes.get(k).next
  }

  // `link(previous, node, next)`.
  pure def link(s: ListState, prv: Link, k: Handle, nxt: Link): ListState = {
    val s1 = { ...s, nodes: s.nodes.put(k, { ...s.nodes.get(k), prev: prv, next: nxt }) }
    val s2 = match prv {
      | Nil => { ...s1, first: To(k) }
      | To(p) => setNext(s1, p, To(k))
    }
    match nxt {
      | Nil => { ...s2, last: To(k) }
      | To(n) => setPrev(s2, n, To(k))
    }
  }

  // `unlink(node)`, which also clears the node's own links (`take()`).
  pure def unlink(s: ListState, k: Handle): ListState = {
    val prv = s.nodes.get(k).prev
    val nxt = s.nodes.get(k).next
    val s0 = { ...s, nodes: s.nodes.put(k, { ...s.nodes.get(k), prev: Nil, next: Nil }) }
    val s1 = match prv {
      | Nil => { ...s0, first: nxt }
      | To(p) => setNext(s0, p, nxt)
    }
    match nxt {
      | Nil => { ...s1, last: prv }
      | To(n) => setPrev(s1, n, prv)
    }
  }

  pure def insertBefore(s: ListState, k: Handle, nxt: Link): ListState = link(s, prevOf(s, nxt), k, nxt)

  pure def insertAfter(s: ListState, prv: Link, k: Handle): ListState = link(s, prv, k, nextOf(s, prv))

  // `allocate(value)`: a new unlinked node with the next number of list `l`.
  pure def allocate(s: ListState, l: int, v: int): Made = {
    val h = { list: l, n: s.nextId }
    { s: { ...s, nodes: s.nodes.put(keyOf(h), { prev: Nil, next: Nil, value: v }), nextId: s.nextId + 1 }, h: h }
  }

  pure def valueAt(s: ListState, k: Handle): int = s.nodes.get(k).value

  // `shift_while(start, shift, check)`: walk while the peeked node passes `v <= bound`.
  pure def shiftWhile(s: ListState, start: Handle, d: Dir, bound: int): Handle = {
    val walked = range(0, MaxLen + 1).foldl({ cur: start, done: false }, (acc, _) =>
      if (acc.done) acc
      else {
        val peek = if (d == Forward) s.nodes.get(acc.cur).next else s.nodes.get(acc.cur).prev
        match peek {
          | Nil => { ...acc, done: true }
          | To(p) => if (valueAt(s, p) <= bound) { cur: p, done: false } else { ...acc, done: true }
        }
      })
    walked.cur
  }

  // Walk the links from the first node (or the last, backward), at most one step
  // more than the node count, so that a cycle shows up as a too-long walk.
  pure def walk(s: ListState, d: Dir): List[Handle] = {
    val start = if (d == Forward) s.first else s.last
    val walked = range(0, s.nodes.keys().size() + 1).foldl({ out: List(), cur: start }, (acc, _) =>
      match acc.cur {
        | Nil => acc
        | To(k) =>
          if (s.nodes.keys().contains(k))
            { out: acc.out.append(k), cur: if (d == Forward) s.nodes.get(k).next else s.nodes.get(k).prev }
          else { out: acc.out.append(k), cur: Nil }
      })
    walked.out
  }

  pure def remaining(s: ListState, d: Dir): Link = {
    val start = if (d == Forward) s.first else s.last
    val walked = range(0, s.nodes.keys().size()).foldl(start, (cur, _) =>
      match cur {
        | Nil => Nil
        | To(k) => if (s.nodes.keys().contains(k)) (if (d == Forward) s.nodes.get(k).next else s.nodes.get(k).prev) else Nil
      })
    walked
  }

  // --- the ghost order, as Pekko's operations change it -----------------------

  // A ghost entry keeps the handle and the value the operation was asked to
  // store, independently of the links, so that a value stored or read wrong on
  // the links side shows up.
  pure def holds(g: List[GEntry], h: Handle): bool = g.indices().exists(i => g[i].h == h)

  pure def indexOf(g: List[GEntry], h: Handle): int = g.indices().filter(i => g[i].h == h).fold(-1, (a, i) => i)

  pure def insertAt(g: List[GEntry], i: int, e: GEntry): List[GEntry] =
    g.slice(0, i).append(e).concat(g.slice(i, g.length()))

  pure def without(g: List[GEntry], h: Handle): List[GEntry] = g.select(x => x.h != h)

  pure def reverse(g: List[Handle]): List[Handle] = g.foldl(List(), (acc, x) => List(x).concat(acc))

  pure def keysOf(g: List[GEntry]): List[Handle] = g.foldl(List(), (acc, e) => acc.append(keyOf(e.h)))

  pure def valuesOf(g: List[GEntry]): List[int] = g.foldl(List(), (acc, e) => acc.append(e.v))

  // The ghost's `find_*` walk: from `i`, step while the neighbour's ghost value
  // is at most `bound`.
  pure def ghostStop(g: List[GEntry], i: int, d: Dir, bound: int): int =
    range(0, MaxLen).foldl(i, (j, _) =>
      if (d == Forward) (if (j + 1 < g.length() and g[j + 1].v <= bound) j + 1 else j)
      else (if (j - 1 >= 0 and g[j - 1].v <= bound) j - 1 else j))

  // --- actions ----------------------------------------------------------------

  pure val Empty: ListState = { nodes: Map(), first: Nil, last: Nil, nextId: 0 }

  action init = all {
    lists' = Lists.mapBy(_ => Empty),
    ghost' = Lists.mapBy(_ => List()),
    issued' = Set(),
    issueCount' = 0,
    resultOk' = true,
  }

  // A new node goes in; `s2` is the list after the links change and `g2` the
  // ghost order after the same operation.
  action commitNew(l: int, made: Made, s2: ListState, g2: List[GEntry]): bool = all {
    lists' = lists.set(l, s2),
    ghost' = ghost.set(l, g2),
    issued' = issued.union(Set(made.h)),
    issueCount' = issueCount + 1,
    resultOk' = true,
  }

  // No new node; `ok` says whether the result agrees with the ghost semantics.
  action commitSame(l: int, s2: ListState, g2: List[GEntry], ok: bool): bool = all {
    lists' = lists.set(l, s2),
    ghost' = ghost.set(l, g2),
    issued' = issued,
    issueCount' = issueCount,
    resultOk' = ok,
  }

  // The links and the ghost disagree on what the operation does. The state is
  // kept and the failure is recorded, rather than disabling the step, which
  // would hide the disagreement.
  action disagree: bool = all {
    lists' = lists,
    ghost' = ghost,
    issued' = issued,
    issueCount' = issueCount,
    resultOk' = false,
  }

  action prependValue(l: int, v: int): bool = {
    val s = lists.get(l)
    val made = allocate(s, l, v)
    all {
      ghost.get(l).length() < MaxLen,
      commitNew(l, made, insertBefore(made.s, keyOf(made.h), s.first), List({ h: made.h, v: v }).concat(ghost.get(l))),
    }
  }

  action appendValue(l: int, v: int): bool = {
    val s = lists.get(l)
    val made = allocate(s, l, v)
    all {
      ghost.get(l).length() < MaxLen,
      commitNew(l, made, insertAfter(made.s, s.last, keyOf(made.h)), ghost.get(l).append({ h: made.h, v: v })),
    }
  }

  // `remove(h)` returns the value of the node; it must be the ghost's value.
  action removeNode(l: int, h: Handle): bool = {
    val s = lists.get(l)
    val g = ghost.get(l)
    val got = s.nodes.get(keyOf(h)).value
    val s1 = unlink(s, keyOf(h))
    all {
      holds(g, h),
      commitSame(l, { ...s1, nodes: mapRemove(s1.nodes, keyOf(h)) }, without(g, h), got == g[indexOf(g, h)].v),
    }
  }

  action moveToFront(l: int, h: Handle): bool = {
    val s = lists.get(l)
    val g = ghost.get(l)
    val k = keyOf(h)
    val s1 = unlink(s, k)
    val s2 = if (s.first == To(k)) s else insertBefore(s1, k, s1.first)
    all {
      holds(g, h),
      commitSame(l, s2, List(g[indexOf(g, h)]).concat(without(g, h)), true),
    }
  }

  action moveToBack(l: int, h: Handle): bool = {
    val s = lists.get(l)
    val g = ghost.get(l)
    val k = keyOf(h)
    val s1 = unlink(s, k)
    val s2 = if (s.last == To(k)) s else insertAfter(s1, s1.last, k)
    all {
      holds(g, h),
      commitSame(l, s2, without(g, h).append(g[indexOf(g, h)]), true),
    }
  }

  // `get_first_or_else_prepend(|v| v == c, || v)` and `get_last_or_else_append`.
  // The links and the ghost each decide whether the end passes `check`.
  action getEndOrElseInsert(l: int, d: Dir, c: int, v: int): bool = {
    val s = lists.get(l)
    val g = ghost.get(l)
    val end = if (d == Forward) s.first else s.last
    val linkPasses = match end {
      | Nil => false
      | To(k) => valueAt(s, k) == c
    }
    val gEnd = if (d == Forward) 0 else g.length() - 1
    val ghostPasses = if (g.length() == 0) false else g[gEnd].v == c
    if (linkPasses and ghostPasses) {
      val got = match end { | Nil => { list: -1, n: -1 } | To(k) => k }
      commitSame(l, s, g, got == keyOf(g[gEnd].h))
    } else if (not(linkPasses) and not(ghostPasses)) {
      val made = allocate(s, l, v)
      val s2 = if (d == Forward) insertBefore(made.s, keyOf(made.h), s.first) else insertAfter(made.s, s.last, keyOf(made.h))
      val g2 = if (d == Forward) List({ h: made.h, v: v }).concat(g) else g.append({ h: made.h, v: v })
      all { g.length() < MaxLen, commitNew(l, made, s2, g2) }
    } else {
      disagree
    }
  }

  // `find_next_or_else_insert(h, |v| v <= bound, |v| v == c, || v)` and the
  // backward one. `bound = -1` never passes, which is
  // `get_next_or_else_insert` / `get_previous_or_else_insert`. The links and
  // the ghost each walk and each decide whether the neighbour passes.
  action findOrElseInsert(l: int, d: Dir, h: Handle, bound: int, c: int, v: int): bool = {
    val s = lists.get(l)
    val g = ghost.get(l)
    val j = ghostStop(g, indexOf(g, h), d, bound)
    val neighbour = if (d == Forward) j + 1 else j - 1
    val ghostHit = if (neighbour >= 0 and neighbour < g.length()) g[neighbour].v == c else false
    // Links: shift_while, then the neighbour of the node it stopped at.
    val start = shiftWhile(s, keyOf(h), d, bound)
    val peek = if (d == Forward) s.nodes.get(start).next else s.nodes.get(start).prev
    val linkHit = match peek {
      | Nil => false
      | To(p) => valueAt(s, p) == c
    }
    all {
      holds(g, h),
      if (linkHit and ghostHit) {
        val got = match peek { | Nil => { list: -1, n: -1 } | To(p) => p }
        commitSame(l, s, g, got == keyOf(g[neighbour].h))
      } else if (not(linkHit) and not(ghostHit)) {
        val made = allocate(s, l, v)
        val s2 = if (d == Forward) insertAfter(made.s, To(start), keyOf(made.h)) else insertBefore(made.s, keyOf(made.h), To(start))
        val g2 = if (d == Forward) insertAt(g, j + 1, { h: made.h, v: v }) else insertAt(g, j, { h: made.h, v: v })
        all { g.length() < MaxLen, commitNew(l, made, s2, g2) }
      } else {
        disagree
      },
    }
  }

  action step = {
    nondet l = Lists.oneOf()
    nondet v = Values.oneOf()
    nondet c = Values.oneOf()
    nondet bound = Set(-1, 0, 1, 2).oneOf()
    nondet d = Set(Forward, Backward).oneOf()
    val g = ghost.get(l)
    any {
      prependValue(l, v),
      appendValue(l, v),
      getEndOrElseInsert(l, d, c, v),
      all {
        g.length() > 0,
        nondet i = g.indices().oneOf()
        any {
          removeNode(l, g[i].h),
          moveToFront(l, g[i].h),
          moveToBack(l, g[i].h),
          findOrElseInsert(l, d, g[i].h, bound, c, v),
        },
      },
    }
  }

  // --- invariants -------------------------------------------------------------

  // The ends are both empty or both set, exactly when the list has no nodes.
  val endsMatchEmptiness: bool = Lists.forall(l => {
    val s = lists.get(l)
    (s.first == Nil) == (s.nodes.keys().size() == 0) and (s.last == Nil) == (s.nodes.keys().size() == 0)
  })

  // `n.next == m` exactly when `m.prev == n`; only the first node has no
  // previous node and only the last has no next node.
  val linksAgree: bool = Lists.forall(l => {
    val s = lists.get(l)
    s.nodes.keys().forall(k => {
      val e = s.nodes.get(k)
      and {
        match e.next {
          | Nil => s.last == To(k)
          | To(m) => s.nodes.keys().contains(m) and s.nodes.get(m).prev == To(k)
        },
        match e.prev {
          | Nil => s.first == To(k)
          | To(p) => s.nodes.keys().contains(p) and s.nodes.get(p).next == To(k)
        },
      }
    })
  })

  // Walking forward visits every node once and ends; walking backward is the
  // reverse; both are the ghost order, and the values met on the way are the
  // ghost's values. The length is the node count.
  val walksAreTheGhostOrder: bool = Lists.forall(l => {
    val s = lists.get(l)
    val expected = keysOf(ghost.get(l))
    val walked = walk(s, Forward)
    and {
      walked == expected,
      walked.foldl(List(), (acc, k) => acc.append(if (s.nodes.keys().contains(k)) s.nodes.get(k).value else -1))
        == valuesOf(ghost.get(l)),
      walk(s, Backward) == reverse(expected),
      remaining(s, Forward) == Nil,
      remaining(s, Backward) == Nil,
      s.nodes.keys().size() == ghost.get(l).length(),
    }
  })

  // A handle names a node of a list exactly when the ghost order of that list
  // holds it: a removed handle names nothing, and a handle of one list never
  // names a node of another list.
  val handlesNameExactlyTheGhostNodes: bool = Lists.forall(l =>
    issued.forall(h => found(lists.get(l), h) == holds(ghost.get(l), h)))

  // No handle is returned for two new nodes.
  val handlesAreNeverReused: bool = issued.size() == issueCount

  // Every operation returned what the ghost semantics names (the node, and the
  // value for `remove`), and the links and the ghost never disagreed.
  val resultsMatchTheGhost: bool = resultOk

  val inv: bool = and {
    endsMatchEmptiness,
    linksAgree,
    walksAreTheGhostOrder,
    handlesNameExactlyTheGhostNodes,
    handlesAreNeverReused,
    resultsMatchTheGhost,
  }
}
