// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2016-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The set of setups an actor system is created with.

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::any::{Any, TypeId};
use core::fmt;

use crate::kernel::setup::Setup;

#[cfg(test)]
#[path = "actor_system_setup_test.rs"]
mod tests;

/// An immutable set of [`Setup`] values, at most one of each concrete type.
///
/// Mirrors `org.apache.pekko.actor.setup.ActorSystemSetup`. Pekko keys each
/// setup by its runtime class; the port keys it by the [`TypeId`] of its
/// concrete type. The values are shared, so cloning a setup copies no setup
/// value.
#[derive(Clone, Default)]
pub struct ActorSystemSetup {
  setups: BTreeMap<TypeId, Arc<dyn Setup>>,
}

impl ActorSystemSetup {
  /// Returns a setup holding no setup values.
  ///
  /// Mirrors `ActorSystemSetup.empty`, and `ActorSystemSetup()` or the Java
  /// `ActorSystemSetup.create()` called with no setups.
  pub fn empty() -> Self {
    Self::default()
  }

  /// Returns a setup holding each of `setups`, keyed by its concrete type.
  ///
  /// Mirrors `ActorSystemSetup(settings*)` and the Java `create(settings*)`.
  /// When two of `setups` have the same concrete type, the later one is kept.
  pub fn from_setups<I>(setups: I) -> Self
  where
    I: IntoIterator<Item = Arc<dyn Setup>>,
  {
    let setups = setups.into_iter().map(|setup| (Any::type_id(setup.as_ref()), setup)).collect();
    Self { setups }
  }

  /// Returns the setup of concrete type `T`, if this set holds one.
  ///
  /// Mirrors both `get[T]` and the Java `get(Class<T>)`.
  pub fn get<T: Setup>(&self) -> Option<&T> {
    let setup: &dyn Any = self.setups.get(&TypeId::of::<T>())?.as_ref();
    setup.downcast_ref::<T>()
  }

  /// Returns this set with `setup` added, replacing the setup of the same
  /// concrete type if there is one.
  ///
  /// Mirrors `withSetup`.
  pub fn with_setup<T: Setup>(mut self, setup: T) -> Self {
    self.setups.insert(TypeId::of::<T>(), Arc::new(setup));
    self
  }

  /// Returns this set with `setup` added, as [`with_setup`](Self::with_setup)
  /// does.
  ///
  /// Mirrors `and`, the alias for `withSetup` that allows `a and b and c`.
  pub fn and<T: Setup>(self, setup: T) -> Self {
    self.with_setup(setup)
  }
}

impl fmt::Debug for ActorSystemSetup {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.debug_struct("ActorSystemSetup").field("len", &self.setups.len()).finish()
  }
}
