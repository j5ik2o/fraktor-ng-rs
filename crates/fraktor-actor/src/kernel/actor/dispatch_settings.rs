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

//! Dispatcher and mailbox definitions, given as typed values.
//!
//! Mirrors the dispatcher and mailbox resolution of
//! `org.apache.pekko.dispatch.Mailboxes`: a dispatcher names a mailbox,
//! a mailbox may be an alias of another, and the stash capacity falls
//! back mailbox, then dispatcher, then the default (design N1).

mod capacity_limit;
mod dispatch_resolution_error;
mod dispatch_settings_value;
mod mailbox_definition;

pub use capacity_limit::CapacityLimit;
pub use dispatch_resolution_error::DispatchResolutionError;
pub use dispatch_settings_value::DispatchSettings;
pub use mailbox_definition::MailboxDefinition;
