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

//! The deployer: typed sections in, deployments out, lookup by path.

use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

use super::default_resizer::DefaultResizer;
use super::deploy_error::DeployError;
use super::deploy_value::Deploy;
use super::deployed_router::DeployedRouter;
use super::deployment_section::DeploymentSection;
use super::group_kind::GroupKind;
use super::number_of_instances::NumberOfInstances;
use super::resizer_section::ResizerSection;
use super::router_selection::RouterSelection;
use super::scope::Scope;
use crate::kernel::actor::InvalidActorNameException;
use crate::kernel::actor::PoolKind;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::wildcard_index::WildcardIndex;

#[cfg(test)]
#[path = "deployer_value_test.rs"]
mod tests;

/// The deployer: the deployments of a system, read from typed
/// sections.
///
/// Mirrors `org.apache.pekko.actor.Deployer`, narrowed to building
/// from typed sections, registering a [`Deploy`], and looking one up.
/// The original holds the index in an `AtomicReference` and swaps it
/// with a compare-and-set; the port holds the immutable index as a
/// value and replaces it through `&mut self`, which the cases (a
/// single-threaded lookup) cannot tell apart. Wiring the result into
/// actor creation is a later stage and is not built here.
#[derive(Debug)]
pub struct Deployer {
  index: WildcardIndex<Deploy>,
}

impl Deployer {
  /// Builds a deployer from the key and section of each deployment.
  ///
  /// Mirrors the original's constructor, which parses each section of
  /// `pekko.actor.deployment` and deploys the result. The key
  /// `"default"` is skipped, as the original's constructor branches on
  /// it; a user-supplied default section is not layered over the
  /// others (design of #195, section 2.2, decision 1). When the same
  /// key appears twice, the later section replaces the earlier one
  /// whole, which no HOCON input can produce.
  ///
  /// Note (the split trap): keys are split by the rule of
  /// [`split_deployment_path`], so a key without a leading `/` such
  /// as `"a/b"` shrinks to `/b`, and a one-element key such as `"svc"`
  /// splits to nothing and is silently not registered. That is the
  /// original's behaviour, not an error.
  ///
  /// # Errors
  ///
  /// Returns the [`DeployError`] of the first section that fails to
  /// interpret or to register, in the order given.
  pub fn from_sections(sections: impl IntoIterator<Item = (String, DeploymentSection)>) -> Result<Self, DeployError> {
    let mut deployer = Self { index: WildcardIndex::new() };
    for (key, section) in sections {
      if key == "default" {
        continue;
      }
      let router = interpret(&section, &key)?;
      let deploy = Deploy {
        path: key,
        dispatcher: section.dispatcher.clone(),
        mailbox: section.mailbox.clone(),
        section,
        router,
        scope: Scope::NoScopeGiven,
      };
      deployer.deploy(deploy)?;
    }
    Ok(deployer)
  }

  /// Registers an already interpreted deployment.
  ///
  /// Mirrors `Deployer.deploy`: every element of the path is
  /// validated first, and only then is the deployment inserted into
  /// the index. An empty element fails with the deployment-specific
  /// message of the original, and an element the index refuses (a
  /// double wildcard used as a suffix) fails after validation.
  ///
  /// # Errors
  ///
  /// Returns [`DeployError::InvalidName`] for an empty or invalid
  /// element and [`DeployError::Index`] for a pattern the index
  /// refuses.
  pub fn deploy(&mut self, deploy: Deploy) -> Result<(), DeployError> {
    let elements = split_deployment_path(&deploy.path);
    for element in &elements {
      if element.is_empty() {
        return Err(DeployError::InvalidName(InvalidActorNameException::invalid_name(alloc::format!(
          "Actor name in deployment [{}] must not be empty",
          deploy.path
        ))));
      }
      ActorPath::validate_path_element_in_path(element, &deploy.path)
        .map_err(|message| DeployError::InvalidName(InvalidActorNameException::invalid_name(message)))?;
    }
    let refs: Vec<&str> = elements.iter().map(String::as_str).collect();
    self.index = self.index.insert(&refs, deploy).map_err(DeployError::Index)?;
    Ok(())
  }

  /// Looks up the deployment for a sequence of path elements.
  ///
  /// Mirrors `Deployer.lookup(path: Iterable[String])`.
  #[must_use]
  pub fn lookup(&self, elements: &[&str]) -> Option<&Deploy> {
    self.index.find(elements)
  }

  /// Looks up the deployment for a path string.
  ///
  /// The path is split by [`split_deployment_path`], the same rule
  /// the original's tests apply with `service.split("/").drop(1)`.
  #[must_use]
  pub fn lookup_path(&self, path: &str) -> Option<&Deploy> {
    let elements = split_deployment_path(path);
    let refs: Vec<&str> = elements.iter().map(String::as_str).collect();
    self.lookup(&refs)
  }
}

/// Splits a deployment path the way Java's `path.split("/").drop(1)`
/// does: trailing empty strings are dropped, and then the first
/// element is dropped.
///
/// This is a JVM-specific behaviour of the original, kept as one
/// function so `deploy` and `lookup_path` split alike (design of
/// #195, section 2.3).
fn split_deployment_path(path: &str) -> Vec<String> {
  if path.is_empty() {
    // Java: "".split("/") is [""], and drop(1) leaves nothing.
    return Vec::new();
  }
  let mut parts: Vec<&str> = path.split('/').collect();
  while parts.len() > 1 && parts.last() == Some(&"") {
    parts.pop();
  }
  parts.into_iter().skip(1).map(String::from).collect()
}

/// Reads the count of a section, for the kinds that read it.
///
/// # Errors
///
/// Returns [`DeployError::WrongType`] for a value that is not a
/// 32-bit integer, as the original's `getInt` does.
fn read_count(section: &DeploymentSection) -> Result<i32, DeployError> {
  match &section.nr_of_instances {
    | NumberOfInstances::Value(value) => Ok(*value),
    | NumberOfInstances::NotANumber(value) => {
      Err(DeployError::WrongType { key: "nr-of-instances", value: value.clone() })
    },
  }
}

/// Builds the resizer of a section, for the kinds that read it.
///
/// # Errors
///
/// Returns [`DeployError::InvalidResizer`] when the values fail the
/// constructor's validation.
fn read_resizer(section: &DeploymentSection) -> Result<Option<DefaultResizer>, DeployError> {
  match &section.resizer {
    | None => Ok(None),
    | Some(ResizerSection {
      lower_bound,
      upper_bound,
      pressure_threshold,
      rampup_rate,
      backoff_threshold,
      backoff_rate,
      messages_per_resize,
    }) => DefaultResizer::new(
      *lower_bound,
      *upper_bound,
      *pressure_threshold,
      *rampup_rate,
      *backoff_threshold,
      *backoff_rate,
      *messages_per_resize,
    )
    .map(Some),
  }
}

/// Reads a duration of a section the way the original's `Config`
/// constructors do: as whole milliseconds, dropping any part below
/// one millisecond (`getMillisDuration`). This applies only while a
/// section is interpreted; a router value built from code keeps the
/// duration it was given.
fn whole_millis(duration: Duration) -> Duration {
  Duration::from_millis(duration.as_secs().saturating_mul(1_000).saturating_add(u64::from(duration.subsec_millis())))
}

/// Interprets a section into the router of its deployment.
///
/// Mirrors `Deployer.createRouterConfig` with the `Config`
/// constructors of each router kind: each kind reads only the fields
/// its constructor reads. Pools read the count; the resizer is read
/// by every pool except balancing; groups read only their paths and
/// their own values; `from-code` reads nothing.
fn interpret(section: &DeploymentSection, key: &str) -> Result<DeployedRouter, DeployError> {
  let pool = |kind: PoolKind, resizer: Option<DefaultResizer>, use_pool_dispatcher: bool| {
    Ok(DeployedRouter::Pool { kind, nr_of_instances: read_count(section)?, resizer, use_pool_dispatcher })
  };
  match &section.router {
    | RouterSelection::FromCode => Ok(DeployedRouter::NoRouter),
    | RouterSelection::RoundRobinPool => {
      pool(PoolKind::RoundRobin, read_resizer(section)?, section.pool_dispatcher_given)
    },
    | RouterSelection::RandomPool => pool(PoolKind::Random, read_resizer(section)?, section.pool_dispatcher_given),
    | RouterSelection::BalancingPool => pool(PoolKind::Balancing, None, false),
    | RouterSelection::SmallestMailboxPool => {
      pool(PoolKind::SmallestMailbox, read_resizer(section)?, section.pool_dispatcher_given)
    },
    | RouterSelection::BroadcastPool => {
      pool(PoolKind::Broadcast, read_resizer(section)?, section.pool_dispatcher_given)
    },
    | RouterSelection::ScatterGatherPool => pool(
      PoolKind::ScatterGatherFirstCompleted { within: whole_millis(section.within) },
      read_resizer(section)?,
      section.pool_dispatcher_given,
    ),
    | RouterSelection::TailChoppingPool => pool(
      PoolKind::TailChopping {
        within: whole_millis(section.within),
        interval: whole_millis(section.tail_chopping_interval),
      },
      read_resizer(section)?,
      section.pool_dispatcher_given,
    ),
    | RouterSelection::ConsistentHashingPool => pool(
      PoolKind::ConsistentHashing { virtual_nodes_factor: 0 },
      read_resizer(section)?,
      section.pool_dispatcher_given,
    ),
    | RouterSelection::RoundRobinGroup => Ok(group(GroupKind::RoundRobin, section)),
    | RouterSelection::RandomGroup => Ok(group(GroupKind::Random, section)),
    | RouterSelection::BroadcastGroup => Ok(group(GroupKind::Broadcast, section)),
    | RouterSelection::ScatterGatherGroup => {
      Ok(group(GroupKind::ScatterGatherFirstCompleted { within: whole_millis(section.within) }, section))
    },
    | RouterSelection::TailChoppingGroup => Ok(group(
      GroupKind::TailChopping {
        within: whole_millis(section.within),
        interval: whole_millis(section.tail_chopping_interval),
      },
      section,
    )),
    | RouterSelection::ConsistentHashingGroup => {
      Ok(group(GroupKind::ConsistentHashing { virtual_nodes_factor: 0 }, section))
    },
    | RouterSelection::Custom(name) => {
      Err(DeployError::UnsupportedRouter { selection: name.clone(), key: String::from(key) })
    },
  }
}

fn group(kind: GroupKind, section: &DeploymentSection) -> DeployedRouter {
  DeployedRouter::Group { kind, paths: section.routees_paths.clone() }
}
