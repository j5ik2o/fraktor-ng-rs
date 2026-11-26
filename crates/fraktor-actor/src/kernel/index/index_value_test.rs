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

//! Supplementary unit tests for [`Index`](super::Index).
//!
//! Mirrors `org.apache.pekko.util.Index`.
//!
//! These pin behaviours the ported spec does not exercise: comparator
//! equality (not `Eq`) decides set membership and the deduplication of
//! `values`, and `remove_key` on a missing key returns `None`.

use alloc::{string::String, vec::Vec};

use super::Index;

#[test]
fn membership_uses_the_comparator_not_eq() {
  // The comparator compares absolute values, so 1 and -1 are the same member.
  let index: Index<String, i32> = Index::new(4, |a: &i32, b: &i32| a.abs().cmp(&b.abs()));
  assert!(index.put("k".into(), 1));
  assert!(!index.put("k".into(), -1));
  assert_eq!(index.value_iterator(&"k".into()), Vec::from([1]));
  assert!(index.remove(&"k".into(), &-1));
  assert!(index.is_empty());
}

#[test]
fn values_deduplicates_by_the_comparator() {
  let index: Index<String, i32> = Index::new(4, |a: &i32, b: &i32| a.abs().cmp(&b.abs()));
  index.put("a".into(), 1);
  index.put("b".into(), -1);
  index.put("b".into(), 2);
  // -1 is comparator-equal to the earlier 1, so it is dropped.
  assert_eq!(index.values(), Vec::from([1, 2]));
}

#[test]
fn remove_key_on_a_missing_key_returns_none() {
  let index: Index<String, i32> = Index::new(4, |a: &i32, b: &i32| a.cmp(b));
  assert_eq!(index.remove_key(&"missing".into()), None);
  index.put("k".into(), 1);
  assert_eq!(index.remove_key(&"missing".into()), None);
  assert_eq!(index.remove_key(&"k".into()), Some(Vec::from([1])));
}

#[test]
fn snapshots_do_not_observe_later_writes() {
  let index: Index<String, i32> = Index::new(4, |a: &i32, b: &i32| a.cmp(b));
  index.put("k".into(), 1);
  let snapshot = index.value_iterator(&"k".into());
  index.put("k".into(), 2);
  assert_eq!(snapshot, Vec::from([1]));
  assert_eq!(index.value_iterator(&"k".into()), Vec::from([1, 2]));
}

// Regression test for the PR #229 review: values and keys taken out by the
// remove methods and `clear` must be dropped after the lock is released, so
// a value whose `Drop` calls back into the same index does not deadlock on
// the non-reentrant spin lock. The watchdog turns a deadlock into a failure.
#[test]
fn removed_values_are_dropped_after_the_lock_is_released() {
  use std::{
    sync::{mpsc::channel, Arc, Weak},
    thread,
    time::Duration,
  };

  struct CallsBack {
    index: Weak<Index<String, CallsBack>>,
  }

  impl Drop for CallsBack {
    fn drop(&mut self) {
      if let Some(index) = self.index.upgrade() {
        let _ = index.is_empty();
      }
    }
  }

  let (sender, receiver) = channel();
  thread::spawn(move || {
    let index = Arc::new(Index::new(4, |a: &CallsBack, b: &CallsBack| {
      let _ = (a, b);
      core::cmp::Ordering::Equal
    }));
    let weak = Arc::downgrade(&index);
    index.put("k".into(), CallsBack { index: weak.clone() });
    assert!(index.remove(&"k".into(), &CallsBack { index: weak.clone() }));
    index.put("k".into(), CallsBack { index: weak.clone() });
    index.remove_value(&CallsBack { index: weak.clone() });
    index.put("k".into(), CallsBack { index: weak.clone() });
    assert!(index.remove_key(&"k".into()).is_some());
    index.put("k".into(), CallsBack { index: weak });
    index.clear();
    let _ = sender.send(());
  });
  receiver.recv_timeout(Duration::from_secs(30)).expect("a removed value was dropped under the lock");
}
