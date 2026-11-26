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

//! The seal that keeps the actor name spellings inside this crate.

/// The supertrait of [`ActorNameArg`](super::ActorNameArg).
///
/// The trait is public so it can bound a public trait, but its module is
/// private, so code outside this crate cannot name it or implement it.
/// Only the spellings this crate provides implement `ActorNameArg`.
pub trait ActorNameArgSeal {}
