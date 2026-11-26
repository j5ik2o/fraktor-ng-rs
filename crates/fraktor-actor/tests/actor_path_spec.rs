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

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/actor/ActorPathSpec.scala
// First ported from v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff).
// v2.0.0 adds three cases after "validate path elements": "accept valid path
// elements", "reject invalid path elements and report the position" and
// "classify every char like the reference predicate" (upstream e336453c66,
// #3542).
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_path::RootActorPath;
use fraktor_actor::kernel::address::Address;
use fraktor_actor::kernel::address::AddressFromUriString;

fn address(protocol: &str, system: &str) -> Address {
  Address::new(protocol, system)
}

fn address_with(protocol: &str, system: &str, host: &str, port: u16) -> Address {
  Address::with_host_port(protocol, system, host, port)
}

// "An ActorPath" must { "support parsing its String rep" in { ... } }
#[test]
fn support_parsing_its_string_rep() {
  let path = RootActorPath::new(address("pekko", "mysys")).with_child("user");
  assert_eq!(ActorPath::from_string(&path.to_string()), Some(path));
}

// "An ActorPath" must { "support parsing remote paths" in { ... } }
#[test]
fn support_parsing_remote_paths() {
  let remote = "pekko://my_sys@host:1234/some/ref";
  assert_eq!(ActorPath::from_string(remote).unwrap().to_string(), remote);
}

// "An ActorPath" must { "support parsing remote akka paths" in { ... } }
#[test]
fn support_parsing_remote_akka_paths() {
  let remote = "akka://my_sys@host:1234/some/ref";
  assert_eq!(ActorPath::from_string(remote).unwrap().to_string(), remote);
}

// "An ActorPath" must { "support parsing AddressFromURIString" in { ... } }
#[test]
fn support_parsing_address_from_uri_string() {
  let remote = "pekko://my_sys@host:1234";
  assert_eq!(AddressFromUriString::apply(remote), address_with("pekko", "my_sys", "host", 1234));
}

// "An ActorPath" must { "support parsing akka AddressFromURIString" in { ... } }
#[test]
fn support_parsing_akka_address_from_uri_string() {
  let remote = "akka://my_sys@host:1234";
  assert_eq!(AddressFromUriString::apply(remote), address_with("akka", "my_sys", "host", 1234));
}

// "An ActorPath" must { "throw exception upon malformed paths" in { ... } }
#[test]
fn throw_exception_upon_malformed_paths() {
  assert!(ActorPath::from_string("").is_none());
  assert!(ActorPath::from_string("://hallo").is_none());
  assert!(ActorPath::from_string("s://dd@:12").is_none());
  assert!(ActorPath::from_string("s://dd@h:hd").is_none());
  assert!(ActorPath::from_string("a://l:1/b").is_none());
}

// "An ActorPath" must { "create correct toString" in { ... } }
#[test]
fn create_correct_to_string() {
  let a = address("pekko", "mysys");
  assert_eq!(RootActorPath::new(a.clone()).to_string(), "pekko://mysys/");
  assert_eq!(RootActorPath::new(a.clone()).with_child("user").to_string(), "pekko://mysys/user");
  assert_eq!(RootActorPath::new(a.clone()).with_child("user").with_child("foo").to_string(), "pekko://mysys/user/foo");
  assert_eq!(
    RootActorPath::new(a.clone()).with_child("user").with_child("foo").with_child("bar").to_string(),
    "pekko://mysys/user/foo/bar"
  );
}

// "An ActorPath" must { "have correct path elements" in { ... } }
#[test]
fn have_correct_path_elements() {
  let path = RootActorPath::new(address("pekko", "mysys")).with_child("user").with_child("foo").with_child("bar");
  assert_eq!(path.elements(), vec!["user".to_string(), "foo".to_string(), "bar".to_string()]);
}

// "An ActorPath" must { "create correct toStringWithoutAddress" in { ... } }
#[test]
fn create_correct_to_string_without_address() {
  let a = address("pekko", "mysys");
  assert_eq!(RootActorPath::new(a.clone()).to_string_without_address(), "/");
  assert_eq!(RootActorPath::new(a.clone()).with_child("user").to_string_without_address(), "/user");
  assert_eq!(
    RootActorPath::new(a.clone()).with_child("user").with_child("foo").to_string_without_address(),
    "/user/foo"
  );
  assert_eq!(
    RootActorPath::new(a.clone()).with_child("user").with_child("foo").with_child("bar").to_string_without_address(),
    "/user/foo/bar"
  );
}

// "An ActorPath" must { "validate path elements" in { ... } }
#[test]
fn validate_path_elements() {
  let err = ActorPath::validate_path_element("");
  assert!(err.is_err());
  assert!(err.unwrap_err().contains("must not be empty"));
}

// "An ActorPath" must { "accept valid path elements" in { ... } }
//
// The original's `forAll(Table(...))` is a loop over the same names.
// `validatePathElement` returns normally for a valid name; the port's returns
// `Ok(())`.
#[test]
fn accept_valid_path_elements() {
  let long = "x".repeat(10000);
  let valid = [
    "a",
    "actor-1",
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
    "-_.*$+:@&=,!~';",
    "a$b",
    "%20",
    "%ff%FF%0a",
    "a%20b",
    long.as_str(),
  ];
  for name in valid {
    assert!(ActorPath::is_valid_path_element(name), "name [{name}]");
    assert_eq!(ActorPath::validate_path_element(name), Ok(()), "name [{name}]");
  }
}

// "An ActorPath" must { "reject invalid path elements and report the position" in { ... } }
//
// The original reads the position from the `InvalidActorNameException`
// message; the port reads it from the `Err` message, which has the same
// "at position: <n>" text. Java counts positions in UTF-16 units and the port
// in chars. They agree here because every character before the reported one is
// ASCII: the first character that is not ASCII is already invalid.
#[test]
fn reject_invalid_path_elements_and_report_the_position() {
  fn position_of(name: &str) -> usize {
    let message = ActorPath::validate_path_element(name).expect_err("the name is invalid");
    match message.split("at position: ").nth(1) {
      | Some(rest) => rest.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap(),
      | None => panic!("unexpected message [{message}]"),
    }
  }

  let long = format!("{}\u{00e9}", "x".repeat(100));
  let invalid = [
    ("$a", 0), // `$` is reserved for system names at the start
    ("a b", 1),
    ("a/b", 1),
    ("a#b", 1),
    ("a?b", 1),
    ("a%", 1), // `%` is only valid as `%XX`
    ("a%2", 1),
    ("a%2g", 1),
    ("a%g2", 1),
    ("caf\u{00e9}", 3), // Latin-1 but not ASCII
    ("na\u{00ef}ve-actor", 2),
    ("\u{4e2d}\u{6587}", 0), // outside Latin-1
    ("actor-\u{4e2d}", 6),
    ("actor-\u{1f600}", 6), // surrogate pair in UTF-16
    ("a\u{0000}b", 1),
    ("a\u{007f}b", 1),
    (long.as_str(), 100),
  ];
  for (name, position) in invalid {
    assert!(!ActorPath::is_valid_path_element(name), "name [{name}]");
    assert_eq!(position_of(name), position, "name [{name}]");
  }
}

// "An ActorPath" must { "classify every char like the reference predicate" in { ... } }
//
// The original walks all 65,536 UTF-16 code units, including the surrogates
// U+D800 to U+DFFF. A Rust `str` cannot hold a lone surrogate, so the port
// walks the other 63,488 and leaves the surrogates out. A surrogate pair is
// covered by "reject invalid path elements and report the position" above.
#[test]
fn classify_every_char_like_the_reference_predicate() {
  // Independent, exhaustive definition of the accepted alphabet, so the
  // validator is locked to it for every char.
  fn is_valid_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_uppercase() || c.is_ascii_digit() || "-_.*$+:@&=,!~';".contains(c)
  }
  fn is_hex_char(c: char) -> bool {
    ('a'..='f').contains(&c) || ('A'..='F').contains(&c) || c.is_ascii_digit()
  }

  for code in 0_u32..=0xFFFF {
    let Some(ch) = char::from_u32(code) else {
      continue;
    };
    let clue = format!("char U+{code:04X}: ");
    assert_eq!(ActorPath::is_valid_path_element(&ch.to_string()), is_valid_char(ch) && ch != '$', "{clue}");
    assert_eq!(ActorPath::is_valid_path_element(&format!("a{ch}")), is_valid_char(ch), "{clue}");
    assert_eq!(ActorPath::is_valid_path_element(&format!("%{ch}0")), is_hex_char(ch), "{clue}");
    assert_eq!(ActorPath::is_valid_path_element(&format!("%0{ch}")), is_hex_char(ch), "{clue}");
  }
}

// "An ActorPath" must { "create correct toStringWithAddress" in { ... } }
#[test]
fn create_correct_to_string_with_address() {
  let local = address("pekko", "mysys");
  let a = address_with("pekko", "mysys", "aaa", 7355);
  let b = address_with("pekko", "mysys", "bb", 7355);
  let c = address_with("pekko", "mysys", "cccc", 7355);
  let root = RootActorPath::new(local);
  assert_eq!(root.to_string_with_address(&a), "pekko://mysys@aaa:7355/");
  assert_eq!(root.with_child("user").to_string_with_address(&a), "pekko://mysys@aaa:7355/user");
  assert_eq!(root.with_child("user").with_child("foo").to_string_with_address(&a), "pekko://mysys@aaa:7355/user/foo");

  // root.toStringWithAddress(b) === "pekko://mysys@bb:7355/"
  assert_eq!(root.with_child("user").to_string_with_address(&b), "pekko://mysys@bb:7355/user");
  assert_eq!(root.with_child("user").with_child("foo").to_string_with_address(&b), "pekko://mysys@bb:7355/user/foo");

  assert_eq!(root.to_string_with_address(&c), "pekko://mysys@cccc:7355/");
  assert_eq!(root.with_child("user").to_string_with_address(&c), "pekko://mysys@cccc:7355/user");
  assert_eq!(root.with_child("user").with_child("foo").to_string_with_address(&c), "pekko://mysys@cccc:7355/user/foo");

  let root_a = RootActorPath::new(a);
  assert_eq!(root_a.to_string_with_address(&b), "pekko://mysys@aaa:7355/");
  assert_eq!(root_a.with_child("user").to_string_with_address(&b), "pekko://mysys@aaa:7355/user");
  assert_eq!(root_a.with_child("user").with_child("foo").to_string_with_address(&b), "pekko://mysys@aaa:7355/user/foo");
}

// "An ActorPath" must { "not allow path separators in RootActorPath's name" in { ... } }
#[test]
fn not_allow_path_separators_in_root_actor_path_name() {
  let err = RootActorPath::with_name(address("pekko", "mysys"), "/user/boom/*");
  assert!(err.is_err());
  assert!(err.unwrap_err().contains("is a path separator"));

  // check that creating such path still works
  assert!(ActorPath::from_string("pekko://mysys/user/boom/*").is_some());
}

// "An ActorPath" must { "detect valid and invalid chars in host names when not using
// AddressFromURIString, e.g. docker host given name" in { ... } }
#[test]
fn detect_valid_and_invalid_chars_in_host_names() {
  let valid = [
    address_with("pekko", "sys", "valid", 0),
    address_with("pekko", "sys", "is_valid.org", 0),
    address_with("pekko", "sys", "fu.is_valid.org", 0),
  ];
  assert!(valid.iter().all(|a| !a.has_invalid_host_characters()));

  let invalid = [address_with("pekko", "sys", "in_valid", 0), address_with("pekko", "sys", "invalid._org", 0)];
  assert!(invalid.iter().all(|a| a.has_invalid_host_characters()));

  assert!(AddressFromUriString::try_apply("pekko://sys@in_valid:5001").is_none());
}

// "An ActorPath" must { "not fail fast if the check is called on valid chars in host names" in { ... } }
#[test]
fn not_fail_fast_if_the_check_is_called_on_valid_chars_in_host_names() {
  let valid = [
    address_with("pekko", "sys", "localhost", 0),
    address_with("pekko", "sys", "is_valid.org", 0),
    address_with("pekko", "sys", "fu.is_valid.org", 0),
  ];
  for a in valid {
    assert!(a.check_host_characters().is_ok());
  }
}

// "An ActorPath" must { "fail fast if the check is called when invalid chars are in host names" in { ... } }
#[test]
fn fail_fast_if_the_check_is_called_when_invalid_chars_are_in_host_names() {
  let valid = [
    address_with("pekko", "sys", "localhost", 0),
    address_with("pekko", "sys", "is_valid.org", 0),
    address_with("pekko", "sys", "fu.is_valid.org", 0),
  ];
  for a in valid {
    assert!(a.check_host_characters().is_ok());
  }

  assert!(address_with("pekko", "sys", "in_valid", 0).check_host_characters().is_err());
  assert!(address_with("pekko", "sys", "invalid._org", 0).check_host_characters().is_err());
}
