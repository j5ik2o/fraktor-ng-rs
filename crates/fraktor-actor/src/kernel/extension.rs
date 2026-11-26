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

//! Actor system extensions: values created once per system, keyed by the
//! type of their id.
//!
//! Mirrors `org.apache.pekko.actor.Extension`, `ExtensionId` and the
//! extension registry of `ActorSystem`. Pekko loads extensions named by
//! class-name strings in its configuration; the port takes typed ids
//! instead (see [`ExtensionStartup`]), so naming an extension that does not
//! exist is a compile error rather than a load failure.

mod extension_entry;
mod extension_error;
mod extension_id;
mod extension_startup;
mod extension_trait;

pub use extension_error::ExtensionError;
pub use extension_id::ExtensionId;
pub use extension_startup::ExtensionStartup;
pub use extension_trait::Extension;

pub(crate) use extension_entry::ExtensionEntry;

#[cfg(test)]
#[path = "extension_test.rs"]
mod tests;
