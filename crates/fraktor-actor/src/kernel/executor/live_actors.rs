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

//! The actors of a system by path, for a selection to resolve on the
//! sender's thread.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;

use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::sync::LockCell;
use crate::kernel::sync::LockFactory;

/// The live actors of one system, keyed by path.
///
/// Pekko's `ActorSelection.tell` resolves the path on the sender's thread
/// (`ActorSelection.deliverSelection`) and tells the actor it finds, so a
/// message the same sender sends to that actor directly afterwards comes
/// after it. A sender inside `receive` holds the executor, so a selection
/// reads this table, under a lock of its own, instead.
pub(crate) struct LiveActors {
  actors: LockCell<BTreeMap<String, ActorRef>>,
}

impl LiveActors {
  pub(crate) fn new(locks: &dyn LockFactory) -> Self {
    Self { actors: LockCell::new(locks.new_lock(), BTreeMap::new()) }
  }

  /// Records `actor` under its path.
  pub(crate) fn insert(&self, actor: ActorRef) {
    let key = actor.path().to_string();
    let replaced = self.actors.with(|actors| actors.insert(key, actor));
    drop(replaced);
  }

  /// Forgets `actor`, when it is the one recorded under its path.
  pub(crate) fn remove(&self, actor: &ActorRef) {
    let key = actor.path().to_string();
    let removed = self.actors.with(|actors| match actors.get(&key) {
      | Some(recorded) if recorded == actor => actors.remove(&key),
      | _ => None,
    });
    drop(removed);
  }

  /// Returns the actor at `path`.
  pub(crate) fn find(&self, path: &ActorPath) -> Option<ActorRef> {
    let key = path.to_string();
    self.actors.with(|actors| actors.get(&key).cloned())
  }

  /// Forgets every actor, for a system that terminates.
  pub(crate) fn clear(&self) {
    let cleared = self.actors.with(core::mem::take);
    drop(cleared);
  }
}
