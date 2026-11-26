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

//! The extensions a system is asked to create when it starts, as typed
//! values.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::kernel::actor_system::ActorSystem;
use crate::kernel::extension::{ExtensionError, ExtensionId};

/// One erased registration step of [`ExtensionStartup`].
type RegisterStep = Box<dyn FnOnce(&mut ActorSystem) -> Result<(), ExtensionError>>;

/// The extensions a system creates at startup, handed to
/// [`ActorSystem::with_extensions`] as a value.
///
/// Mirrors the two configuration columns `pekko.extensions` and
/// `pekko.library-extensions`, which Pekko reads as class-name strings and
/// loads by reflection. The port takes typed ids instead: only the id of
/// an extension that exists can be listed, so the original's "cannot be
/// loaded" failures have no equivalent state. That is the alternative
/// example for the two ExtensionSpec cases proposed as not built:
///
/// ```compile_fail
/// use fraktor_actor::kernel::extension::ExtensionStartup;
///
/// struct NotAnExtensionId;
///
/// // `NotAnExtensionId` does not implement `ExtensionId`, so it cannot be
/// // listed: naming an extension that does not exist does not compile.
/// let _startup = ExtensionStartup::new().extension(NotAnExtensionId);
/// ```
///
/// The two columns keep the order Pekko starts them in: the library
/// column is processed first, then the listed column. With typed input
/// the columns no longer differ in how a failure is handled, so the order
/// is the only difference that remains.
pub struct ExtensionStartup {
  library: Vec<RegisterStep>,
  listed: Vec<RegisterStep>,
}

impl ExtensionStartup {
  /// Creates an empty startup list: the system starts with no extensions
  /// registered.
  pub fn new() -> Self {
    Self { library: Vec::new(), listed: Vec::new() }
  }

  /// Adds `id` to the listed column (Pekko's `pekko.extensions`): the
  /// extension is created when the system starts, and a failure fails the
  /// system's creation.
  pub fn extension<E: ExtensionId>(mut self, id: E) -> Self {
    self.listed.push(Box::new(move |system: &mut ActorSystem| system.register_extension(&id).map(|_| ())));
    self
  }

  /// Adds `id` to the library column (Pekko's `pekko.library-extensions`):
  /// the column a library uses to offer its own extension. The library
  /// column is processed before the listed column.
  pub fn library_extension<E: ExtensionId>(mut self, id: E) -> Self {
    self.library.push(Box::new(move |system: &mut ActorSystem| system.register_extension(&id).map(|_| ())));
    self
  }

  /// Runs the library column first and the listed column second against
  /// `system`, stopping at the first failure.
  pub(crate) fn run(self, system: &mut ActorSystem) -> Result<(), ExtensionError> {
    for step in self.library {
      step(system)?;
    }
    for step in self.listed {
      step(system)?;
    }
    Ok(())
  }
}

impl Default for ExtensionStartup {
  fn default() -> Self {
    Self::new()
  }
}
