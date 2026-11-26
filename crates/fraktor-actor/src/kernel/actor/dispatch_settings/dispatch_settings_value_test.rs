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

//! Unit tests for [`DispatchSettings`](super::DispatchSettings).
//!
//! Mirrors the capacity resolution Pekko's `Mailboxes` performs,
//! including the alias chains of `ActorWithBoundedStashSpec`.

use core::num::NonZeroUsize;

use super::super::{CapacityLimit, DispatchResolutionError, MailboxDefinition};
use super::DispatchSettings;

fn limited(bound: usize) -> CapacityLimit {
  CapacityLimit::Limited(NonZeroUsize::new(bound).expect("a positive bound"))
}

#[test]
fn capacity_limit_from_count_reads_zero_as_unlimited() {
  assert_eq!(CapacityLimit::from_count(0), CapacityLimit::Unlimited);
  assert_eq!(CapacityLimit::from_count(3), limited(3));
  assert!(CapacityLimit::Unlimited.allows(usize::MAX));
  assert!(limited(1).allows(0));
  assert!(!limited(1).allows(1));
}

#[test]
fn undefined_names_resolve_to_none_not_an_error() {
  let settings = DispatchSettings::new();
  assert_eq!(settings.resolve_dispatcher("missing"), Ok(None));
  assert_eq!(settings.resolve_mailbox("missing"), Ok(None));
  assert_eq!(
    settings.resolve_capacities(Some("missing"), None),
    Ok((CapacityLimit::Unlimited, CapacityLimit::Unlimited))
  );
}

#[test]
fn aliases_resolve_through_two_steps() {
  let mut settings = DispatchSettings::new();
  settings.define_dispatcher("d1", MailboxDefinition::bounded(10, 20));
  settings.alias_dispatcher("a1", "d1");
  settings.alias_dispatcher("a2", "a1");
  assert_eq!(settings.resolve_dispatcher("a2"), Ok(Some(MailboxDefinition::bounded(10, 20))));
}

#[test]
fn alias_cycle_and_unknown_target_and_depth_are_errors() {
  let mut settings = DispatchSettings::new();
  settings.alias_dispatcher("x", "y");
  settings.alias_dispatcher("y", "x");
  assert!(matches!(settings.resolve_dispatcher("x"), Err(DispatchResolutionError::AliasCycle { .. })));
  let mut settings = DispatchSettings::new();
  settings.alias_dispatcher("x", "missing");
  assert!(matches!(settings.resolve_dispatcher("x"), Err(DispatchResolutionError::AliasTargetUnknown { .. })));
  let mut settings = DispatchSettings::new();
  settings.define_dispatcher("d0", MailboxDefinition::bounded(1, 1));
  for i in 0..25 {
    settings.alias_dispatcher(&alloc::format!("a{i}"), &alloc::format!("a{}", i + 1));
  }
  settings.alias_dispatcher("a25", "d0");
  assert!(matches!(settings.resolve_dispatcher("a0"), Err(DispatchResolutionError::AliasTooDeep { .. })));
}

#[test]
fn stash_capacity_falls_back_but_mailbox_capacity_does_not() {
  // Re-review N1 of #192: the dispatcher defines a bounded mailbox of
  // 10, the mailbox defines only a stash capacity of 20. The original
  // gives the actor an unlimited mailbox and a stash capacity of 20.
  let mut settings = DispatchSettings::new();
  settings.define_dispatcher("d", MailboxDefinition::bounded(10, 99));
  settings.define_mailbox("m", MailboxDefinition::new(None, Some(limited(20))));
  assert_eq!(settings.resolve_capacities(Some("d"), Some("m")), Ok((CapacityLimit::Unlimited, limited(20))));
  // Without the mailbox, both capacities come from the dispatcher.
  assert_eq!(settings.resolve_capacities(Some("d"), None), Ok((limited(10), limited(99))));
  // The stash capacity alone falls back mailbox -> dispatcher.
  let mut settings = DispatchSettings::new();
  settings.define_dispatcher("d", MailboxDefinition::new(None, Some(limited(7))));
  settings.define_mailbox("m", MailboxDefinition::new(Some(limited(3)), None));
  assert_eq!(settings.resolve_capacities(Some("d"), Some("m")), Ok((limited(3), limited(7))));
}

#[test]
fn later_registration_wins_between_definition_and_alias() {
  // A name is either a definition or an alias: defining a name drops
  // its alias, and aliasing a name drops its definition, in both
  // orders and for both kinds.
  let mut settings = DispatchSettings::new();
  settings.define_dispatcher("t", MailboxDefinition::bounded(3, 3));
  settings.define_dispatcher("d", MailboxDefinition::new(None, None));
  settings.alias_dispatcher("d", "t");
  assert_eq!(settings.resolve_dispatcher("d"), Ok(Some(MailboxDefinition::bounded(3, 3))));
  settings.define_dispatcher("d", MailboxDefinition::bounded(7, 7));
  assert_eq!(settings.resolve_dispatcher("d"), Ok(Some(MailboxDefinition::bounded(7, 7))));

  // Mailboxes have no aliases in the original (re-review 2, low 1
  // of #192): a later mailbox definition simply replaces the
  // earlier one, and an undefined mailbox name resolves to none.
  let mut settings = DispatchSettings::new();
  settings.define_mailbox("m", MailboxDefinition::bounded(3, 3));
  settings.define_mailbox("m", MailboxDefinition::bounded(7, 7));
  assert_eq!(settings.resolve_mailbox("m"), Ok(Some(MailboxDefinition::bounded(7, 7))));
  assert_eq!(settings.resolve_mailbox("t"), Ok(None));
}
