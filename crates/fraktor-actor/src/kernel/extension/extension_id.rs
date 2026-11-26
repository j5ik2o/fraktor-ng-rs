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

//! The marker of an extension value and the id that creates it.

use core::any::Any;

use crate::kernel::actor_system::ActorSystem;
use crate::kernel::extension::ExtensionError;

/// A value an actor system creates once and shares through its registry.
///
/// Mirrors `org.apache.pekko.actor.Extension`. The trait is a marker: the
/// registry only needs the value to be a concrete, shareable type.
pub trait Extension: Any + Send + Sync {}

/// The id of an extension: the key the registry counts it under, and the
/// factory that creates it.
///
/// Mirrors `org.apache.pekko.actor.ExtensionId`. Extensions are keyed by
/// the id's **type**: two values of the same id type are the same id, and
/// the second registration returns the instance the first created. An id
/// that counts its creations therefore carries the counter inside the id
/// value, so that tests running in parallel in one binary do not
/// contaminate each other the way a global counter would.
pub trait ExtensionId: Any + Send + Sync {
  /// The extension this id creates.
  type Ext: Extension;

  /// Creates the extension for `system`.
  ///
  /// Called at most once per system for this id type: a failure is kept
  /// and returned to later requests instead of being retried, as Pekko
  /// keeps the `Throwable` a creation raised. The system is handed over
  /// as `&mut`, so a creation may itself start actors (including named
  /// top-level actors through
  /// [`ActorSystem::actor_of_with_name`]) and register other extensions.
  ///
  /// Requesting this same id again from inside the creation fails with
  /// an in-progress error instead of recursing. If the creation panics,
  /// the panic propagates to the caller and the id stays marked as being
  /// created: later requests for it return the in-progress error, not
  /// the panic's content. Pekko keeps the thrown exception and rethrows
  /// it, so the content a later caller sees differs there.
  fn create_extension(&self, system: &mut ActorSystem) -> Result<Self::Ext, ExtensionError>;
}
