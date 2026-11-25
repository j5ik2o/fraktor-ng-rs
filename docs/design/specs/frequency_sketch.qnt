// A model of the counter matrix in crates/fraktor-actor/src/kernel/frequency_sketch
// (PR #205): `CounterMatrix`, which both `FrequencySketch` and
// `FastFrequencySketch` keep, and the increment and frequency of the sketches
// on top of it.
//
// The sketches differ only in how a value becomes a position in each row (a
// hash). The model does not compute hashes: each value gets one position per
// row, chosen freely in `init`, so every placement, including collisions and
// positions beyond the width, is explored. The hashes themselves are pinned to
// the JVM by the unit tests (see the design document).
//
// The 64-bit words are unpacked: a row is the list of its counters, word by
// word, `Slots` counters per word. A position picks its word and its counter
// in the word as `CounterMatrix::locate` does: the word by the position masked
// to the width and shifted past the counters in one word, the counter by the
// low bits of the position alone (not masked to the width).
//
// The invariants say that counters stay in their range, that each row's size
// is the sum of its counters (the odd-counter adjustment of the reset keeps
// this exact), that the sketch's size covers every row's size and stays below
// the reset size, that a value's frequency is its smallest counter, and,
// through a ghost of the true counts halved at every reset, that no counter
// that has never saturated, and no value whose counters have never saturated,
// is underestimated.
//
// `Bug` selects a broken variant for the negative controls in
// frequency_sketch_test.qnt: "none" is the code.
module frequency_sketch {
  // The number of rows (`SketchDepth`).
  const Depth: int
  // The number of counters per row (`SketchWidth`), a positive power of two.
  const Width: int
  // The number of counters per 64-bit word, 64 / counter bits.
  const Slots: int
  // The largest value a counter holds (`CounterBits::max_value`).
  const MaxCount: int
  // The size at which the reset runs (`ResetSize`). Zero or less disables it.
  const ResetSize: int
  // "none", "noOddAdjust", "noSaturation", "staleSize", "firstRowSize",
  // "countSaturated" or "maxFrequency".
  const Bug: str

  pure val Values: Set[int] = Set(0, 1, 2, 3)
  pure val Rows: Set[int] = 0.to(Depth - 1)
  // Positions reach past both the width and one word, so that the masking of
  // the word index and of the counter index are both exercised.
  pure val Positions: Set[int] = 0.to(2 * (if (Width > Slots) Width else Slots) - 1)
  pure val RowWidth: int = if (Width / Slots > 1) Width / Slots else 1
  pure val Cells: int = RowWidth * Slots

  // The counters of each row, word by word.
  var counters: List[List[int]]
  // `row_sizes`.
  var rowSizes: List[int]
  // `updated_size`, what `size()` returns.
  var updatedSize: int
  // The position of each value in each row, as its hash would give.
  var place: (int, int) -> int
  // The true number of increments of each value, halved at every reset.
  var ghost: int -> int
  // The counters, as (row, index in the row), that an increment ever met at
  // their maximum.
  var saturated: Set[(int, int)]

  // --- positions -------------------------------------------------------------

  // `locate`: the word and the counter in the word, as one index in the row.
  pure def cellOf(p: int): int = ((p % Width) / Slots) * Slots + (p % Slots)

  def cellOfValue(v: int, r: int): int = cellOf(place.get((v, r)))

  // --- helpers -----------------------------------------------------------------

  pure def sumOf(l: List[int]): int = l.foldl(0, (acc, x) => acc + x)

  pure def maxOf(l: List[int], otherwise: int): int =
    if (l.length() == 0) otherwise else l.foldl(l[0], (acc, x) => if (x > acc) x else acc)

  pure def oddCount(l: List[int]): int = l.foldl(0, (acc, x) => if (x % 2 == 1) acc + 1 else acc)

  pure def halved(l: List[int]): List[int] = l.foldl(List(), (acc, x) => acc.append(x / 2))

  pure def zeros(n: int): List[int] = range(0, n).foldl(List(), (acc, _i) => acc.append(0))

  // `frequency`: the smallest counter of the value, `i32::MAX` without rows,
  // unless it is broken to take the largest.
  def frequency(v: int): int =
    if (Bug == "maxFrequency")
      range(0, Depth).foldl(0, (acc, r) => {
        val c = counters[r][cellOfValue(v, r)]
        if (c > acc) c else acc
      })
    else
      range(0, Depth).foldl(2147483647, (acc, r) => {
        val c = counters[r][cellOfValue(v, r)]
        if (c < acc) c else acc
      })

  // --- the transition -----------------------------------------------------------

  // Whether `increment` raises the counter: below its maximum, unless the
  // saturation check is broken.
  pure def raises(c: int): bool = Bug == "noSaturation" or c < MaxCount

  // `increment(v)`: raise each row's counter of the value below its maximum,
  // count the update if any row changed, and reset when the size reaches the
  // reset size.
  action increment(v: int): bool = {
    val before = range(0, Depth).foldl(List(), (acc, r) => acc.append(counters[r][cellOfValue(v, r)]))
    val raised = range(0, Depth).foldl(List(), (acc, r) => acc.append(raises(before[r])))
    val rows1 = range(0, Depth).foldl(List(), (acc, r) =>
      acc.append(if (raised[r]) counters[r].replaceAt(cellOfValue(v, r), before[r] + 1) else counters[r]))
    val sizes1 = range(0, Depth).foldl(List(), (acc, r) =>
      acc.append(if (raised[r]) rowSizes[r] + 1 else rowSizes[r]))
    // An update counts when a row changed, unless it is broken to count every
    // update of a sketch with rows, saturated or not.
    val updated = if (Bug == "countSaturated") Depth > 0 else raised.indices().exists(i => raised[i])
    val updatedSize1 = if (updated) updatedSize + 1 else updatedSize
    val ghost1 = ghost.set(v, ghost.get(v) + 1)
    val resets = updated and updatedSize1 == ResetSize
    all {
      saturated' = saturated.union(Rows.filter(r => before[r] >= MaxCount).map(r => (r, cellOfValue(v, r)))),
      place' = place,
      if (resets) {
        // `reset`: halve every counter, and drop each row's size by its odd
        // counters before halving it, then take the largest row size.
        val rows2 = rows1.foldl(List(), (acc, row) => acc.append(halved(row)))
        val sizes2 = range(0, Depth).foldl(List(), (acc, r) =>
          acc.append(if (Bug == "noOddAdjust") sizes1[r] / 2 else (sizes1[r] - oddCount(rows1[r])) / 2))
        all {
          counters' = rows2,
          rowSizes' = sizes2,
          updatedSize' =
            if (Bug == "staleSize") updatedSize1
            else if (Bug == "firstRowSize") sizes2[0]
            else maxOf(sizes2, updatedSize1),
          ghost' = Values.mapBy(x => ghost1.get(x) / 2),
        }
      } else {
        all {
          counters' = rows1,
          rowSizes' = sizes1,
          updatedSize' = updatedSize1,
          ghost' = ghost1,
        }
      },
    }
  }

  action init = {
    nondet p = tuples(Values, Rows).setOfMaps(Positions).oneOf()
    all {
      counters' = range(0, Depth).foldl(List(), (acc, _r) => acc.append(zeros(Cells))),
      rowSizes' = zeros(Depth),
      updatedSize' = 0,
      place' = p,
      ghost' = Values.mapBy(_x => 0),
      saturated' = Set(),
    }
  }

  action step = {
    nondet v = Values.oneOf()
    increment(v)
  }

  // --- invariants ------------------------------------------------------------------

  // Every position lands inside its row.
  val positionsInRange: bool = Positions.forall(p => cellOf(p) >= 0 and cellOf(p) < Cells)

  // Counters stay between zero and their maximum.
  val countersBounded: bool =
    Rows.forall(r => 0.to(Cells - 1).forall(i => counters[r][i] >= 0 and counters[r][i] <= MaxCount))

  // Each row's size is the sum of its counters.
  val rowSizeIsCounterSum: bool = Rows.forall(r => rowSizes[r] == sumOf(counters[r]))

  // The sketch's size covers every row's size: a row changes at most once per
  // counted update, and the reset takes the largest row size.
  val rowSizesWithinSize: bool = Rows.forall(r => rowSizes[r] <= updatedSize)

  // The size stays below an enabled reset size, so the reset runs at the size
  // itself and not later.
  val sizeBelowResetSize: bool = updatedSize >= 0 and (ResetSize < 1 or updatedSize < ResetSize)

  // A sketch without rows counts nothing.
  val noRowsNoSize: bool = Depth > 0 or updatedSize == 0

  // With a single row, the sketch's size is that row's size: an update counts
  // exactly when it raises the row's counter, and the reset takes the row's
  // size.
  val oneRowSizeIsTheSize: bool = Depth != 1 or updatedSize == rowSizes[0]

  // A value's frequency is its smallest counter: no larger than any of its
  // counters, and equal to one of them.
  val frequencyIsTheSmallestCounter: bool = Depth == 0 or Values.forall(v => and {
    Rows.forall(r => frequency(v) <= counters[r][cellOfValue(v, r)]),
    Rows.exists(r => frequency(v) == counters[r][cellOfValue(v, r)]),
  })

  // A counter that has never saturated is at least the halved true counts of
  // the values placed on it, and a value none of whose counters has saturated
  // is not underestimated.
  val noUnderestimateWhileUnsaturated: bool = and {
    Rows.forall(r => 0.to(Cells - 1).forall(i =>
      saturated.contains((r, i))
        or counters[r][i] >= Values.filter(v => cellOfValue(v, r) == i).fold(0, (acc, v) => acc + ghost.get(v)))),
    Values.forall(v =>
      Rows.exists(r => saturated.contains((r, cellOfValue(v, r)))) or frequency(v) >= ghost.get(v)),
  }

  val inv: bool = and {
    positionsInRange,
    countersBounded,
    rowSizeIsCounterSum,
    rowSizesWithinSize,
    sizeBelowResetSize,
    noRowsNoSize,
    oneRowSizeIsTheSize,
    frequencyIsTheSmallestCounter,
    noUnderestimateWhileUnsaturated,
  }
}
