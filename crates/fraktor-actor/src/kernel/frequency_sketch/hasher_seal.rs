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
// Copyright 2015 Ben Manes. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The seal that keeps the frequency sketch hashers inside this crate.

/// The supertrait of [`FrequencySketchHasher`](super::FrequencySketchHasher).
///
/// The trait is public so it can bound a public trait, but its module is
/// private, so code outside this crate cannot name it or implement it. It
/// takes the value type, as `FrequencySketchHasher` does, so each pair of a
/// hasher and a value type is sealed: code outside this crate can neither
/// write a new hasher nor give a hasher of this crate another value type.
/// Only the pairs this crate provides implement `FrequencySketchHasher`, as
/// only the original's own hashers implement its `sealed trait Hasher`.
pub trait HasherSeal<A: ?Sized> {}
