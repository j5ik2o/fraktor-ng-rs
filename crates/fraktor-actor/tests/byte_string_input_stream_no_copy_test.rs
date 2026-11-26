// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// A check that stands in for the excluded case "not copy the backing array" of
// Pekko v2.0.0's UnsynchronizedByteArrayInputStreamSpec (L165-171). It is not
// one of the node's cases and is not counted in the ledger.
//
// The original writes to the array after building the stream and reads the new
// byte back. The port's `ByteString::from_array` copies the bytes, so that
// observation cannot be made. What the port does promise is that building a
// stream from a byte string does not copy its bytes again. This file checks it
// through the public API only, by counting what the global allocator hands out
// while `as_input_stream` runs.
//
// The counting allocator is this test binary's own and the file holds one test,
// so no other test allocates while it measures. The bound is relative to the
// input length (far below one copy) and is not an exact byte count, which the
// API does not promise.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use fraktor_actor::kernel::byte_string::ByteString;

struct CountingAllocator;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call is forwarded to the system allocator unchanged; the
// allocator only adds to a counter.
unsafe impl GlobalAlloc for CountingAllocator {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    ALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
    // SAFETY: the caller's layout is passed on as given.
    unsafe { System.alloc(layout) }
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    // SAFETY: the pointer came from `alloc` above with the same layout.
    unsafe { System.dealloc(ptr, layout) }
  }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn allocated_while<T>(f: impl FnOnce() -> T) -> (T, usize) {
  let before = ALLOCATED.load(Ordering::SeqCst);
  let value = f();
  (value, ALLOCATED.load(Ordering::SeqCst) - before)
}

// Stands in for "not copy the backing array".
#[test]
fn building_a_stream_does_not_copy_the_bytes_of_the_byte_string() {
  const LENGTH: usize = 1 << 20;
  let data = vec![7_u8; LENGTH];
  let single = ByteString::from_array(&data, 0, LENGTH);
  let concatenated = single.concat(&ByteString::from_array(&data, 0, LENGTH));

  for (label, bytes, length) in [("one array", &single, LENGTH), ("two arrays", &concatenated, 2 * LENGTH)] {
    let (mut stream, allocated) = allocated_while(|| bytes.as_input_stream());
    assert!(allocated < length / 16, "{label}: building the stream allocated {allocated} bytes for {length} bytes");
    assert_eq!(stream.available(), length, "{label}");
    assert_eq!(stream.read(), Some(7), "{label}");
  }
}
