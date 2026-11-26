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

//! A port that runs a unit of work and reports whether it failed.

use crate::kernel::supervision::RunOutcome;

/// Runs a piece of work that may fail and reports its outcome.
///
/// Pekko catches the exception that a failing actor throws. The kernel cannot
/// catch a panic without the standard library, so it asks a port to do it and
/// keeps the answer. [`RunOutcome::Completed`] means the work finished;
/// [`RunOutcome::Failed`] means it failed and the parent applies the supervisor
/// strategy, with the failure message standing in for the cause.
pub trait FailureCatcher: Send + Sync {
  /// Runs `body` and returns its outcome.
  fn run(&self, body: &mut dyn FnMut()) -> RunOutcome;
}
