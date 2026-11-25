// What building a frequency sketch allocates (PR #205).
//
// Not a port of a Pekko case and not counted in the ledger. This binary has a
// global allocator that records the largest request, so it holds a single
// test: the harness's own small requests stay below the bound.

use std::{
  alloc::{GlobalAlloc, Layout, System},
  sync::atomic::{AtomicUsize, Ordering},
};

use fraktor_actor::kernel::frequency_sketch::{FrequencySketch, FrequencySketchSettings};

struct LargestRequest;

static LARGEST: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for LargestRequest {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
    unsafe { System.alloc(layout) }
  }

  unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
    LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
    unsafe { System.alloc_zeroed(layout) }
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    unsafe { System.dealloc(ptr, layout) }
  }

  unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    LARGEST.fetch_max(new_size, Ordering::Relaxed);
    unsafe { System.realloc(ptr, layout, new_size) }
  }
}

#[global_allocator]
static ALLOCATOR: LargestRequest = LargestRequest;

// A sketch with no rows counts nothing (I6 of the design: `frequency` is
// `i32::MAX` and `size` stays 0), so it allocates no row of counters. One row
// here would be 2^20 64-bit counters, 8 MiB.
#[test]
fn a_sketch_of_depth_zero_allocates_no_row() {
  let settings = FrequencySketchSettings { width_multiplier: 1, reset_multiplier: 10.0, depth: 0, counter_bits: 64 };
  LARGEST.store(0, Ordering::Relaxed);
  let mut sketch = FrequencySketch::<str>::new(1 << 20, settings).expect("valid settings");
  let largest = LARGEST.load(Ordering::Relaxed);
  assert!(largest < 1 << 20, "building the sketch requested {largest} bytes at once");
  sketch.increment("a");
  assert_eq!(sketch.frequency("a"), i32::MAX);
  assert_eq!(sketch.size(), 0);
}
