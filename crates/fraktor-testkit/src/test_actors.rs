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

//! Common actor patterns that tests use.

mod echo_actor;
mod forward_actor;
mod test_actors_props;

pub use echo_actor::EchoActor;
pub use forward_actor::ForwardActor;
pub use test_actors_props::echo_actor_props;
pub use test_actors_props::forward_actor_props;
