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

//! A gauge that reads one file descriptor value of this process.

/// Which file descriptor value a gauge reads.
#[derive(Clone, Copy)]
enum FileDescriptorKind {
  Open,
  Max,
  Ratio,
}

/// A gauge whose value is read from the operating system when asked.
///
/// Mirrors the gauges of `org.apache.pekko.testkit.metrics.FileDescriptorMetricSet`.
#[derive(Clone)]
pub struct FileDescriptorGauge {
  kind: FileDescriptorKind,
}

impl FileDescriptorGauge {
  /// The gauge of the number of open file descriptors.
  pub(crate) fn open() -> Self {
    Self { kind: FileDescriptorKind::Open }
  }

  /// The gauge of the file descriptor limit.
  pub(crate) fn max() -> Self {
    Self { kind: FileDescriptorKind::Max }
  }

  /// The gauge of the ratio of open descriptors to the limit.
  pub(crate) fn ratio() -> Self {
    Self { kind: FileDescriptorKind::Ratio }
  }

  /// Returns the current value, `NaN` where the platform has no value.
  pub fn value(&self) -> f64 {
    match self.kind {
      | FileDescriptorKind::Open => open_file_descriptors().map(|count| count as f64).unwrap_or(f64::NAN),
      | FileDescriptorKind::Max => max_file_descriptors().map(|limit| limit as f64).unwrap_or(f64::NAN),
      | FileDescriptorKind::Ratio => match (open_file_descriptors(), max_file_descriptors()) {
        | (Some(open), Some(limit)) if limit > 0 => open as f64 / limit as f64,
        | _ => f64::NAN,
      },
    }
  }
}

/// The number of file descriptors this process has open.
#[cfg(unix)]
fn open_file_descriptors() -> Option<u64> {
  let entries = std::fs::read_dir("/proc/self/fd").or_else(|_| std::fs::read_dir("/dev/fd")).ok()?;
  Some(entries.count() as u64)
}

/// The number of file descriptors this process has open.
#[cfg(not(unix))]
fn open_file_descriptors() -> Option<u64> {
  None
}

/// The limit of file descriptors of this process.
///
/// The conversion to `u64` is the identity on 64-bit targets and a
/// widening on 32-bit ones, where `rlim_t` is 32 bits wide.
#[cfg(unix)]
#[allow(clippy::useless_conversion)]
fn max_file_descriptors() -> Option<u64> {
  let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
  // SAFETY: `limit` is a writable `rlimit` owned by this call, and
  // `getrlimit` only writes to it.
  let result = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) };
  if result == 0 {
    // `rlim_t` is 32 bits wide on some unix targets and 64 on others;
    // convert at this boundary so the value is always a `u64`.
    Some(u64::from(limit.rlim_cur))
  } else {
    None
  }
}

/// The limit of file descriptors of this process.
#[cfg(not(unix))]
fn max_file_descriptors() -> Option<u64> {
  None
}
