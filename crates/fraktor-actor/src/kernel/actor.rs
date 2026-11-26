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

//! Actors, their context, and the recipes that create them.

// Public path fixed by the Pekko port (org.apache.pekko.event.Logging).
#[allow(non_snake_case)]
pub mod Logging;

mod actor_context;
mod actor_selection;
mod actor_trait;
mod all_dead_letters;
mod any_dead_letter;
mod behavior_stack;
mod dead_letter;
mod dead_letter_log_limit;
mod dead_letter_settings;
mod dead_letter_suppression;
mod dead_letters_shared;
mod dropped;
mod fsm;
mod fsm_state;
mod invalid_actor_name_exception;
mod props;
mod suppressed_dead_letter;
mod terminated;
mod unhandled_message;

pub use actor_context::ActorContext;
pub use actor_selection::ActorSelection;
pub(crate) use actor_selection::ActorSelectionMessage;
pub use actor_trait::Actor;
pub use all_dead_letters::AllDeadLetters;
pub use any_dead_letter::AnyDeadLetter;
pub(crate) use behavior_stack::BehaviorStack;
pub(crate) use behavior_stack::Running;
pub use dead_letter::DeadLetter;
pub use dead_letter_log_limit::DeadLetterLogLimit;
pub use dead_letter_settings::DeadLetterSettings;
pub use dead_letter_suppression::DeadLetterSuppression;
pub(crate) use dead_letters_shared::DeadLettersShared;
pub use dropped::Dropped;
pub use fsm::Fsm;
pub use fsm_state::FsmState;
pub use invalid_actor_name_exception::InvalidActorNameException;
pub use props::Props;
pub use suppressed_dead_letter::SuppressedDeadLetter;
pub use terminated::Terminated;
pub use unhandled_message::UnhandledMessage;
