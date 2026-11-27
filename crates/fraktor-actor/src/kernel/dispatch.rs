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

//! Dispatcher definitions and the registry that verifies against them, and
//! mailboxes and what runs them.
//!
//! Every actor of a system has a mailbox, which the dispatcher its caller
//! drives by hand runs.

mod bounded_queue;
mod classified_envelope;
mod control_aware_queue;
mod dispatcher_config;
mod dispatcher_kind;
mod dispatchers;
mod mailbox;
mod mailbox_cell;
mod mailbox_run;
mod mailbox_status;
mod manual_executor;
mod manual_task;
mod message_priority;
mod message_queue;
mod message_queue_factory;
mod next;
mod priority_queue;
mod run_limits;
mod run_outcome;
mod run_report;
mod system_message;
mod throughput;
mod throughput_deadline;
mod unbounded_queue;
mod wake_signal;

pub use bounded_queue::BoundedQueue;
pub use classified_envelope::ClassifiedEnvelope;
pub use control_aware_queue::ControlAwareQueue;
pub use dispatcher_config::DispatcherConfig;
pub use dispatcher_kind::DispatcherKind;
pub use dispatchers::Dispatchers;
pub use dispatchers::DEFAULT_DISPATCHER_ID;
pub(crate) use mailbox::Mailbox;
pub(crate) use mailbox_cell::MailboxCell;
pub(crate) use mailbox_run::MailboxRun;
pub(crate) use manual_executor::ManualExecutor;
pub(crate) use manual_task::ManualTask;
pub use message_priority::MessagePriority;
pub use message_queue::MessageQueue;
pub use message_queue_factory::MessageQueueFactory;
pub(crate) use next::Next;
pub use priority_queue::PriorityQueue;
pub use run_limits::RunLimits;
pub(crate) use run_outcome::RunOutcome;
pub(crate) use run_report::RunReport;
pub(crate) use system_message::SystemMessage;
pub use throughput::Throughput;
pub use throughput_deadline::ThroughputDeadline;
pub use unbounded_queue::UnboundedQueue;
pub use wake_signal::WakeSignal;
