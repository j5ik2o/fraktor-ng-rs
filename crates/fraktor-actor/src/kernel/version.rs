// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2015-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! Version comparison and compatibility checking.

mod fraktor_version;
mod pre_release_kind;
mod release_version;
mod unsupported_fraktor_version;
mod version_parse_error;
mod version_value;

pub use fraktor_version::FraktorVersion;
pub use unsupported_fraktor_version::UnsupportedFraktorVersion;
pub use version_parse_error::VersionParseError;
pub use version_value::Version;
