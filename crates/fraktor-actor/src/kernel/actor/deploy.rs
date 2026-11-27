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

//! Deployment: the typed reading of `pekko.actor.deployment` sections
//! and the lookup of the resulting [`Deploy`] values by actor path.
//!
//! Mirrors `org.apache.pekko.actor.Deployer` and the values it produces
//! (`Deploy`, `Scope`). The original reads HOCON; the port takes typed
//! [`DeploymentSection`] values instead.

mod default_resizer;
mod deploy_error;
mod deploy_value;
mod deployed_router;
mod deployer_value;
mod deployment_section;
mod group_kind;
mod number_of_instances;
mod resizer_section;
mod router_selection;
mod scope;

pub use default_resizer::DefaultResizer;
pub use deploy_error::DeployError;
pub use deploy_value::Deploy;
pub use deployed_router::DeployedRouter;
pub use deployer_value::Deployer;
pub use deployment_section::DeploymentSection;
pub use group_kind::GroupKind;
pub use number_of_instances::NumberOfInstances;
pub use resizer_section::ResizerSection;
pub use router_selection::RouterSelection;
pub use router_selection::BALANCING_POOL_FQCN;
pub use router_selection::BROADCAST_GROUP_FQCN;
pub use router_selection::BROADCAST_POOL_FQCN;
pub use router_selection::CONSISTENT_HASHING_GROUP_FQCN;
pub use router_selection::CONSISTENT_HASHING_POOL_FQCN;
pub use router_selection::NO_ROUTER_FQCN;
pub use router_selection::RANDOM_GROUP_FQCN;
pub use router_selection::RANDOM_POOL_FQCN;
pub use router_selection::ROUND_ROBIN_GROUP_FQCN;
pub use router_selection::ROUND_ROBIN_POOL_FQCN;
pub use router_selection::SCATTER_GATHER_GROUP_FQCN;
pub use router_selection::SCATTER_GATHER_POOL_FQCN;
pub use router_selection::SMALLEST_MAILBOX_POOL_FQCN;
pub use router_selection::TAIL_CHOPPING_GROUP_FQCN;
pub use router_selection::TAIL_CHOPPING_POOL_FQCN;
pub use scope::Scope;
