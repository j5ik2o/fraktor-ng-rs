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

//! Dispatcher and mailbox definitions and aliases, as typed values.

use alloc::collections::BTreeMap;
use alloc::string::String;

use super::{CapacityLimit, DispatchResolutionError, MailboxDefinition};
use crate::kernel::setup::Setup;

#[cfg(test)]
#[path = "dispatch_settings_value_test.rs"]
mod tests;

/// The original's `MaxDispatcherAliasDepth`.
const MAX_ALIAS_DEPTH: usize = 20;

/// Dispatcher and mailbox definitions and aliases, as typed values.
///
/// Mirrors the dispatcher and mailbox sections of Pekko's configuration
/// (the `Mailboxes` resolution), with the HOCON text replaced by values:
/// this is the faithfulness replacement recorded as E2 in the design of
/// #192. The settings travel as a [`Setup`] inside an
/// [`ActorSystemSetup`](crate::kernel::setup::ActorSystemSetup), and the
/// executor resolves the definition of each actor when it is spawned;
/// later changes do not reach actors that already run.
#[derive(Clone, Default)]
pub struct DispatchSettings {
  dispatchers: BTreeMap<String, MailboxDefinition>,
  mailboxes: BTreeMap<String, MailboxDefinition>,
  dispatcher_aliases: BTreeMap<String, String>,
  mailbox_aliases: BTreeMap<String, String>,
}

impl DispatchSettings {
  /// Creates empty settings: every name is undefined and therefore
  /// unlimited, which is the behaviour before this type existed.
  pub fn new() -> Self {
    Self::default()
  }

  /// Defines the dispatcher `name`.
  ///
  /// A name is either a definition or an alias, never both: defining
  /// a name drops an alias of the same name, and aliasing a name
  /// drops its definition. The later registration wins.
  pub fn define_dispatcher(&mut self, name: &str, definition: MailboxDefinition) -> &mut Self {
    self.dispatcher_aliases.remove(name);
    self.dispatchers.insert(String::from(name), definition);
    self
  }

  /// Defines the mailbox `name`.
  ///
  /// A name is either a definition or an alias, never both: defining
  /// a name drops an alias of the same name, and aliasing a name
  /// drops its definition. The later registration wins.
  pub fn define_mailbox(&mut self, name: &str, definition: MailboxDefinition) -> &mut Self {
    self.mailbox_aliases.remove(name);
    self.mailboxes.insert(String::from(name), definition);
    self
  }

  /// Makes the dispatcher name `alias` another name for `target`.
  ///
  /// Aliasing a name drops a definition of the same name, as
  /// [`define_dispatcher`](Self::define_dispatcher) drops an alias:
  /// the later registration wins.
  pub fn alias_dispatcher(&mut self, alias: &str, target: &str) -> &mut Self {
    self.dispatchers.remove(alias);
    self.dispatcher_aliases.insert(String::from(alias), String::from(target));
    self
  }

  /// Makes the mailbox name `alias` another name for `target`.
  ///
  /// Aliasing a name drops a definition of the same name, as
  /// [`define_mailbox`](Self::define_mailbox) drops an alias: the
  /// later registration wins.
  pub fn alias_mailbox(&mut self, alias: &str, target: &str) -> &mut Self {
    self.mailboxes.remove(alias);
    self.mailbox_aliases.insert(String::from(alias), String::from(target));
    self
  }

  /// Resolves the dispatcher `name`, following aliases.
  ///
  /// # Errors
  ///
  /// Returns [`DispatchResolutionError`] for an alias cycle, an alias
  /// whose target is not defined, or a chain longer than 20. A name
  /// that is neither defined nor an alias resolves to `Ok(None)`.
  pub fn resolve_dispatcher(&self, name: &str) -> Result<Option<MailboxDefinition>, DispatchResolutionError> {
    Self::resolve(&self.dispatchers, &self.dispatcher_aliases, name)
  }

  /// Resolves the mailbox `name`, following aliases, as
  /// [`resolve_dispatcher`](Self::resolve_dispatcher) does.
  ///
  /// # Errors
  ///
  /// Returns [`DispatchResolutionError`] under the same conditions.
  pub fn resolve_mailbox(&self, name: &str) -> Result<Option<MailboxDefinition>, DispatchResolutionError> {
    Self::resolve(&self.mailboxes, &self.mailbox_aliases, name)
  }

  fn resolve(
    definitions: &BTreeMap<String, MailboxDefinition>,
    aliases: &BTreeMap<String, String>,
    name: &str,
  ) -> Result<Option<MailboxDefinition>, DispatchResolutionError> {
    let mut current = name;
    let mut visited: alloc::vec::Vec<&str> = alloc::vec![name];
    loop {
      if let Some(definition) = definitions.get(current) {
        return Ok(Some(*definition));
      }
      let Some(target) = aliases.get(current) else {
        if current == name {
          return Ok(None);
        }
        return Err(DispatchResolutionError::AliasTargetUnknown { name: String::from(current) });
      };
      if visited.len() > MAX_ALIAS_DEPTH {
        return Err(DispatchResolutionError::AliasTooDeep { name: String::from(name) });
      }
      if visited.contains(&target.as_str()) {
        return Err(DispatchResolutionError::AliasCycle { name: String::from(name) });
      }
      visited.push(target.as_str());
      current = target;
    }
  }

  /// Resolves the two capacities that govern one actor spawned with
  /// the given dispatcher and mailbox names.
  ///
  /// The stash capacity is the only one that falls back in three steps
  /// — the mailbox definition, then the dispatcher definition, then
  /// unlimited — as the original's `stashCapacityFromConfig` does. The
  /// mailbox capacity does not fall back to the dispatcher when a
  /// mailbox is named: it comes from the mailbox definition alone
  /// (unlimited when that definition does not set one), and the
  /// dispatcher's mailbox definition is used only when no mailbox is
  /// named, as the original's `getMailboxType` does (re-review N1 of
  /// #192).
  ///
  /// # Errors
  ///
  /// Returns [`DispatchResolutionError`] when either name's alias chain
  /// is broken, as the `resolve_*` methods report.
  pub fn resolve_capacities(
    &self,
    dispatcher: Option<&str>,
    mailbox: Option<&str>,
  ) -> Result<(CapacityLimit, CapacityLimit), DispatchResolutionError> {
    let dispatcher_definition = match dispatcher {
      | Some(name) => self.resolve_dispatcher(name)?,
      | None => None,
    };
    let mailbox_definition = match mailbox {
      | Some(name) => self.resolve_mailbox(name)?,
      | None => None,
    };
    let stash_capacity = mailbox_definition
      .and_then(|definition| definition.stash_capacity)
      .or_else(|| dispatcher_definition.and_then(|definition| definition.stash_capacity))
      .unwrap_or(CapacityLimit::Unlimited);
    let mailbox_capacity = if mailbox.is_some() {
      mailbox_definition.and_then(|definition| definition.mailbox_capacity).unwrap_or(CapacityLimit::Unlimited)
    } else {
      dispatcher_definition.and_then(|definition| definition.mailbox_capacity).unwrap_or(CapacityLimit::Unlimited)
    };
    Ok((mailbox_capacity, stash_capacity))
  }
}

impl Setup for DispatchSettings {}
