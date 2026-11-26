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

//! The spellings the name argument of `ActorSystem::actor_of_named`
//! accepts: every reference a caller can hold to a string, and
//! `Option<&str>` with `None` for Pekko's `null` name.

use std::borrow::Cow;
use std::boxed::Box;
use std::string::String;

use fraktor_actor::kernel::actor::Props;
use fraktor_testkit::fraktorspec::FraktorSpec;

#[test]
fn actor_of_named_accepts_every_name_spelling() {
  let mut spec = FraktorSpec::new("ActorOfNamedArg");

  let from_str = spec.system_mut().actor_of_named(Props::empty(), "from-str").expect("a &str name is accepted");
  assert_eq!(from_str.path().name(), "from-str");

  let reference_to_str = &"from-ref-str";
  let from_ref_str =
    spec.system_mut().actor_of_named(Props::empty(), reference_to_str).expect("a &&str name is accepted");
  assert_eq!(from_ref_str.path().name(), "from-ref-str");

  let owned = String::from("worker");
  let from_string_ref = spec.system_mut().actor_of_named(Props::empty(), &owned).expect("a &String name is accepted");
  assert_eq!(from_string_ref.path().name(), "worker");

  let boxed: Box<str> = Box::from("from-box");
  let from_box_ref = spec.system_mut().actor_of_named(Props::empty(), &boxed).expect("a &Box<str> name is accepted");
  assert_eq!(from_box_ref.path().name(), "from-box");

  let cow: Cow<'_, str> = Cow::Borrowed("from-cow");
  let from_cow_ref = spec.system_mut().actor_of_named(Props::empty(), &cow).expect("a &Cow<str> name is accepted");
  assert_eq!(from_cow_ref.path().name(), "from-cow");

  let from_option_str =
    spec.system_mut().actor_of_named(Props::empty(), Some("from-option-str")).expect("an Option<&str> is accepted");
  assert_eq!(from_option_str.path().name(), "from-option-str");

  // A bare `None` resolves to `Option<&str>` and is refused as Pekko's
  // `null` name; the refusal's message is checked by
  // `local_actor_ref_provider_spec`, so only the refusal is checked here.
  let _ = spec.system_mut().actor_of_named(Props::empty(), None).expect_err("a null name is refused");

  spec.terminate();
}
