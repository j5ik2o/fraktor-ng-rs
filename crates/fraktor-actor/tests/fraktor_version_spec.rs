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

// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by f86d79ba3c #2441 (also on 1.x); the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/PekkoVersionSpec.scala
//
// The original's thirteen cases under `"The Pekko version check" must` are kept
// as one test each, named after the case, with the original name as a comment.
// Every `PekkoVersion.require(...)` call from each case runs in the original
// order with the same three arguments, and the original comments are kept.
//
// `PekkoVersion.require(libraryName, requiredVersion, currentVersion)` throws
// `UnsupportedPekkoVersion` on failure and returns normally otherwise. The port's
// `FraktorVersion::require(library_name, required_version, current_version) ->
// Result<(), UnsupportedFraktorVersion>` returns the outcome, so a normal return
// is `assert_eq!(.., Ok(()))` and `intercept[UnsupportedPekkoVersion] { .. }` is
// `assert!(..is_err())`. Following product-surface-naming, `PekkoVersion` is
// renamed to `FraktorVersion` and `UnsupportedPekkoVersion` to
// `UnsupportedFraktorVersion`; the original's library-name data string
// `"PekkoVersionSpec"` is kept verbatim.
//
// The product's `FraktorVersion` lives at `fraktor_actor::kernel::version`. Until
// that module exists the file does not compile: that compile error is the
// intended red for this node.

use fraktor_actor::kernel::version::FraktorVersion;

// "succeed if version is ok"
#[test]
fn succeed_if_version_is_ok() {
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5.6"), Ok(()));
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5.7"), Ok(()));
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.6.0"), Ok(()));
}

// "succeed if version is RC and ok"
#[test]
fn succeed_if_version_is_rc_and_ok() {
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5.7-RC10"), Ok(()));
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.6.0-RC1", "2.6.0-RC1"), Ok(()));
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.6.0-RC1", "2.6.0-RC2"), Ok(()));
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.6.0-RC1", "2.7.0"), Ok(()));
}

// "fail if version is RC and not ok"
#[test]
fn fail_if_version_is_rc_and_not_ok() {
  assert!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5.6-RC1").is_err());
  assert!(FraktorVersion::require("PekkoVersionSpec", "2.5.6-RC2", "2.5.6-RC1").is_err());
}

// "succeed if version is milestone and ok"
#[test]
fn succeed_if_version_is_milestone_and_ok() {
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5.7-M10"), Ok(()));
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.7-M9", "2.5.7-M9"), Ok(()));
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.7-M9", "2.5.7-M10"), Ok(()));
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.7-M9", "2.5.8"), Ok(()));
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.7-M9", "2.5.7-RC1"), Ok(()));
}

// "fail if version is milestone and not ok"
#[test]
fn fail_if_version_is_milestone_and_not_ok() {
  assert!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5.6-M1").is_err());
  assert!(FraktorVersion::require("PekkoVersionSpec", "2.5.7-M10", "2.5.7-M9").is_err());
  assert!(FraktorVersion::require("PekkoVersionSpec", "2.5.7-RC1", "2.5.7-M9").is_err());
}

// "fail if major version is different"
#[test]
fn fail_if_major_version_is_different() {
  // because not bincomp
  assert!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "3.0.0").is_err());
  assert!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "1.0.0").is_err());
}

// "fail if minor version is too low"
#[test]
fn fail_if_minor_version_is_too_low() {
  assert!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.4.19").is_err());
}

// "fail if patch version is too low"
#[test]
fn fail_if_patch_version_is_too_low() {
  assert!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5.5").is_err());
}

// "succeed if current Pekko version is SNAPSHOT"
#[test]
fn succeed_if_current_version_is_snapshot() {
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5-SNAPSHOT"), Ok(()));
}

// "succeed if current Pekko version is timestamped SNAPSHOT"
#[test]
fn succeed_if_current_version_is_timestamped_snapshot() {
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5-20180109-133700"), Ok(()));
}

// "succeed if required Pekko version is SNAPSHOT"
#[test]
fn succeed_if_required_version_is_snapshot() {
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5-SNAPSHOT", "2.5-SNAPSHOT"), Ok(()));
}

// "succeed if required Pekko version is timestamped SNAPSHOT"
#[test]
fn succeed_if_required_version_is_timestamped_snapshot() {
  assert_eq!(FraktorVersion::require("PekkoVersionSpec", "2.5-20180109-133700", "2.5-20180109-133700"), Ok(()));
}

// "silently comply if current version is incomprehensible"
#[test]
fn silently_comply_if_current_version_is_incomprehensible() {
  // because we may want to release with weird numbers for some reason
  assert_eq!(FraktorVersion::require("nonsense", "2.5.6", "nonsense"), Ok(()));
}
