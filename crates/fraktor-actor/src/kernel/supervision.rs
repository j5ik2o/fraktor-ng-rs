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

//! What a parent does with a child that fails.

mod directive;
mod failure_catcher;
mod failure_kind;
mod restart_history;
mod restart_limit;
mod restart_window;
mod run_outcome;
mod supervisor_strategy;

pub use directive::Directive;
pub use failure_catcher::FailureCatcher;
pub use failure_kind::FailureKind;

pub(crate) use restart_history::RestartHistory;
pub use restart_limit::RestartLimit;

pub use restart_window::RestartWindow;
pub use run_outcome::RunOutcome;
pub use supervisor_strategy::SupervisorStrategy;
