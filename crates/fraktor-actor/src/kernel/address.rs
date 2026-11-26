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

//! Address value types.

mod address_from_uri_string;
mod address_value;
mod relative_actor_path;
mod uri_parts;

pub use address_from_uri_string::AddressFromUriString;
pub use address_value::Address;
pub use relative_actor_path::RelativeActorPath;
pub(crate) use uri_parts::UriParts;
