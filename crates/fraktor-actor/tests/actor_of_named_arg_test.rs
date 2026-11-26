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

// The name argument of `ActorSystem::actor_of_named` (issue #194,
// PR #289): a caller that holds the name in a `String` passes `&String`,
// which the earlier `impl Into<Option<&str>>` form refused, because
// trait matching does not apply the `&String` to `&str` deref coercion.
// The accepted spellings are `&str`, `&String`, and `Option<&str>`
// (with `None` for Pekko's `null`). There is no `Option<&String>`
// spelling: a bare `None` must keep resolving to `Option<&str>`, and
// a second `Option` implementation would make that call ambiguous.

use fraktor_actor::kernel::actor::Props;
use fraktor_testkit::fraktorspec::FraktorSpec;

#[test]
fn actor_of_named_accepts_every_name_spelling() {
  let mut spec = FraktorSpec::new("ActorOfNamedArg");

  let from_str = spec.system_mut().actor_of_named(Props::empty(), "from-str").expect("a &str name is accepted");
  assert_eq!(from_str.path().name(), "from-str");

  let owned = String::from("worker");
  let from_string_ref = spec.system_mut().actor_of_named(Props::empty(), &owned).expect("a &String name is accepted");
  assert_eq!(from_string_ref.path().name(), "worker");

  let from_option_str =
    spec.system_mut().actor_of_named(Props::empty(), Some("from-option-str")).expect("an Option<&str> is accepted");
  assert_eq!(from_option_str.path().name(), "from-option-str");

  let null_error = spec.system_mut().actor_of_named(Props::empty(), None).expect_err("a null name is refused");
  assert!(null_error.to_string().contains("null"), "unexpected message: {null_error}");

  spec.terminate();
}
