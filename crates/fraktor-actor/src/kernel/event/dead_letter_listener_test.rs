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
// Licensed under the Apache License, version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Issue #297 (design `docs/design/issue-296-297-dead-letters-followup.md`,
// section 7.2, review high 3): a system message that reaches dead letters is
// reported under the name of its kind, not under the name of the
// `SystemMessage` enum. Pekko's `DeadLetterListener` writes
// `unwrapped.getClass.getName` (`event/DeadLetterListener.scala`, lines 139
// and 140), so the line names `Create`, `Resume`, and so on; the port wraps
// the enum in a `Message`, whose type name is `SystemMessage` for every kind.
// After #297 `SystemMessage` carries `class_name`, and the listener reads it
// for a system message payload (design section 5.4). The counts and the log
// limit are unchanged.
//
// `SystemMessage` is `pub(crate)` and this is a kernel unit test.

use core::time::Duration;

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor::AnyDeadLetter;
use crate::kernel::actor::DeadLetter;
use crate::kernel::actor::DeadLetterLogLimit;
use crate::kernel::actor::DeadLetterSettings;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::dispatch::SystemMessage;
use crate::kernel::event::DeadLetterListener;
use crate::kernel::message::Message;

fn dead_letters_handle() -> ActorRef {
  ActorRef::new(
    0,
    0,
    std::sync::Arc::new(crate::kernel::actor_path::ActorPath::from_root(
      crate::kernel::actor_path::RootActorPath::new(crate::kernel::address::Address::new("pekko", "Listener")),
    )),
    std::sync::Arc::new(ChannelBoundary::new()),
  )
}

fn target() -> ActorRef {
  ActorRef::new(
    1,
    0,
    std::sync::Arc::new(crate::kernel::actor_path::ActorPath::from_root(
      crate::kernel::actor_path::RootActorPath::new(crate::kernel::address::Address::new("pekko", "Listener")),
    )),
    std::sync::Arc::new(ChannelBoundary::new()),
  )
}

fn never_suppressed(_: &Message) -> bool {
  false
}

// A `DeadLetter` carrying `Create` is logged under its kind, not
// `SystemMessage`: the line names `...system_message::Create`.
#[test]
fn a_system_message_dead_letter_is_logged_under_its_kind() {
  let dead_letters = dead_letters_handle();
  let target = target();
  let mut listener =
    DeadLetterListener::new(DeadLetterSettings::new(DeadLetterLogLimit::Unlimited, None), dead_letters.clone());
  let dead = DeadLetter::from_message(Message::new(SystemMessage::Create), target.clone(), target.clone());

  let line = listener.on_event(&AnyDeadLetter::DeadLetter(&dead), Duration::ZERO, &never_suppressed);
  let line = line.expect("the listener logs an unlimited dead letter");
  assert!(
    line.contains("[fraktor_actor::kernel::dispatch::system_message::Create]"),
    "the line names the kind of the system message, not the enum: {line}"
  );
}

// A `DeadLetter` carrying `Resume` is logged under its kind too, since the
// commander decided a `Resume` for a closed mailbox is a plain dead letter
// (design section 6, decision 6).
#[test]
fn a_resume_dead_letter_is_logged_under_its_kind() {
  let dead_letters = dead_letters_handle();
  let target = target();
  let mut listener =
    DeadLetterListener::new(DeadLetterSettings::new(DeadLetterLogLimit::Unlimited, None), dead_letters.clone());
  let dead = DeadLetter::from_message(Message::new(SystemMessage::Resume), target.clone(), target.clone());

  let line = listener.on_event(&AnyDeadLetter::DeadLetter(&dead), Duration::ZERO, &never_suppressed);
  let line = line.expect("the listener logs an unlimited dead letter");
  assert!(
    line.contains("[fraktor_actor::kernel::dispatch::system_message::Resume]"),
    "the line names the kind of the system message, not the enum: {line}"
  );
}

// A `SuppressedDeadLetter` is never logged, whether it carries a suppressed
// kind (`Terminate`) or would name a kind (`ChildTerminated`). The listener
// keeps a suppressed report out of the log, as Pekko's `DeadLetterListener`
// does not subscribe to the suppressed channel.
#[test]
fn a_suppressed_system_message_is_not_logged() {
  use crate::kernel::actor::SuppressedDeadLetter;

  let dead_letters = dead_letters_handle();
  let target = target();
  let mut listener =
    DeadLetterListener::new(DeadLetterSettings::new(DeadLetterLogLimit::Unlimited, None), dead_letters.clone());
  let suppressed =
    SuppressedDeadLetter::from_message(Message::new(SystemMessage::Terminate), target.clone(), dead_letters.clone());

  let line = listener.on_event(&AnyDeadLetter::Suppressed(&suppressed), Duration::ZERO, &never_suppressed);
  assert!(line.is_none(), "a suppressed report is not logged: {line:?}");
}
