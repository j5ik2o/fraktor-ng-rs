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

//! Extracts the path elements of a relative actor path.

use crate::kernel::address::uri_parts::RelativeUriParts;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// Entry point for splitting a relative actor path into its name elements.
#[derive(Clone, Copy, Debug, Default)]
pub struct RelativeActorPath;

impl RelativeActorPath {
  /// Splits `path` into its name elements, or returns `None` when `path`
  /// is not a relative actor path.
  ///
  /// The raw path is split on `/` without dropping empty elements and
  /// without percent-decoding. When a fragment is present it is appended
  /// to the last element after `#`.
  pub fn try_apply(path: &str) -> Option<Vec<String>> {
    let parts = RelativeUriParts::parse(path)?;
    let mut elements: Vec<String> = parts.raw_path().split('/').map(str::to_string).collect();
    if let Some(fragment) = parts.raw_fragment() {
      let last = elements.len() - 1;
      elements[last].push('#');
      elements[last].push_str(fragment);
    }
    Some(elements)
  }
}
