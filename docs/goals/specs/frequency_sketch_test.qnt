// Instances and tests of the model in frequency_sketch.qnt.
//
// The instances follow the shapes the sketches take:
// - `narrow`: 2-bit counters (32 per word, maximum 3) in a row narrower than
//   one word, so the counter index is not masked to the width, and counters
//   saturate early.
// - `fast`: the fixed shape of `FastFrequencySketch`, four rows of 4-bit
//   counters (16 per word, maximum 15), width 16.
// - `wide`: three rows of 8-bit counters (8 per word, maximum 255) over four
//   words, so the word index matters.
// - `noReset`: a reset size of zero, which never resets.
// - `noRows`: depth zero.
// The `bug*` instances are the negative controls. Each must violate `inv`.

module narrow {
  import frequency_sketch(Depth = 2, Width = 4, Slots = 32, MaxCount = 3, ResetSize = 10, Bug = "none").*
    from "./frequency_sketch"

  // Two values on distinct counters: the counts are exact until the maximum.
  run incrementCountersToMaxValue =
    init
      .then(all {
        place' = Set((0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1), (3, 0), (3, 1))
          .mapBy(k => if (k._1 == 0) 1 else 2),
        counters' = counters, rowSizes' = rowSizes, updatedSize' = updatedSize, ghost' = ghost, everSaturated' = everSaturated,
      })
      .then(increment(0))
      .then(increment(0))
      .then(increment(0))
      .then(increment(0))
      .then(increment(0))
      .then(all {
        assert(frequency(0) == 3),
        assert(updatedSize == 3),
        assert(everSaturated),
        assert(inv),
        counters' = counters, rowSizes' = rowSizes, updatedSize' = updatedSize, place' = place, ghost' = ghost,
        everSaturated' = everSaturated,
      })

  // Ten updates on four counters, 3, 3, 3 and 1: the tenth halves them to 1,
  // 1, 1 and 0, and each row's size drops its four odd counters first:
  // (10 - 4) / 2 = 3.
  run resetHalvesAndAdjustsForOddCounters =
    init
      .then(all {
        place' = Set((0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1), (3, 0), (3, 1))
          .mapBy(k => if (k._1 == 3) 3 else k._1),
        counters' = counters, rowSizes' = rowSizes, updatedSize' = updatedSize, ghost' = ghost, everSaturated' = everSaturated,
      })
      .then(increment(0))
      .then(increment(0))
      .then(increment(0))
      .then(increment(1))
      .then(increment(1))
      .then(increment(1))
      .then(increment(2))
      .then(increment(2))
      .then(increment(2))
      .then(all {
        assert(updatedSize == 9),
        assert(everSaturated == false),
        counters' = counters, rowSizes' = rowSizes, updatedSize' = updatedSize, place' = place, ghost' = ghost,
        everSaturated' = everSaturated,
      })
      .then(increment(3))
      .then(all {
        assert(updatedSize == 3),
        assert(rowSizes == List(3, 3)),
        assert(frequency(0) == 1),
        assert(frequency(3) == 0),
        assert(inv),
        counters' = counters, rowSizes' = rowSizes, updatedSize' = updatedSize, place' = place, ghost' = ghost,
        everSaturated' = everSaturated,
      })
}

module fast {
  import frequency_sketch(Depth = 4, Width = 16, Slots = 16, MaxCount = 15, ResetSize = 20, Bug = "none").*
    from "./frequency_sketch"
}

module wide {
  import frequency_sketch(Depth = 3, Width = 32, Slots = 8, MaxCount = 255, ResetSize = 12, Bug = "none").*
    from "./frequency_sketch"
}

module noReset {
  import frequency_sketch(Depth = 2, Width = 4, Slots = 32, MaxCount = 3, ResetSize = 0, Bug = "none").*
    from "./frequency_sketch"
}

module noRows {
  import frequency_sketch(Depth = 0, Width = 4, Slots = 16, MaxCount = 15, ResetSize = 5, Bug = "none").*
    from "./frequency_sketch"
}

module bugNoOddAdjust {
  import frequency_sketch(Depth = 2, Width = 4, Slots = 32, MaxCount = 3, ResetSize = 10, Bug = "noOddAdjust").*
    from "./frequency_sketch"
}

module bugNoSaturation {
  import frequency_sketch(Depth = 2, Width = 4, Slots = 32, MaxCount = 3, ResetSize = 10, Bug = "noSaturation").*
    from "./frequency_sketch"
}

module bugStaleSize {
  import frequency_sketch(Depth = 2, Width = 4, Slots = 32, MaxCount = 3, ResetSize = 10, Bug = "staleSize").*
    from "./frequency_sketch"
}
