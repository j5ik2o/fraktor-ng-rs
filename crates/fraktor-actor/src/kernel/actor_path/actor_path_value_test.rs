//! Tests for the actor path name primitives and [`ActorPath`](super::ActorPath).

use super::ActorPath;
use super::PathElement;
use super::RootActorPath;
use super::RootName;
use crate::kernel::address::Address;

fn address() -> Address {
  Address::new("pekko", "mysys")
}

#[test]
fn path_element_rejects_an_empty_string() {
  assert!(PathElement::new("").is_err(), "an empty path element was accepted");
}

#[test]
fn path_element_rejects_a_path_separator() {
  assert!(PathElement::new("us/er").is_err(), "a path element carrying `/` was accepted");
}

#[test]
fn path_element_rejects_a_fragment_separator() {
  assert!(PathElement::new("us#er").is_err(), "a path element carrying `#` was accepted");
}

#[test]
fn path_element_keeps_a_valid_name() {
  assert_eq!(PathElement::new("user").unwrap().as_str(), "user");
}

#[test]
fn path_element_parses_every_valid_name() {
  assert_eq!(PathElement::parse("user"), Some(PathElement::new("user").unwrap()));
  assert_eq!(PathElement::parse("us/er"), None);
  assert_eq!(PathElement::parse(""), None);
}

#[test]
fn path_element_parses_an_escaped_separator() {
  assert_eq!(PathElement::parse("us%2Fer").unwrap().as_str(), "us%2Fer");
}

#[test]
fn root_name_accepts_a_unit_name() {
  assert_eq!(RootName::new("a").unwrap().as_str(), "a");
  assert_eq!(RootName::new("x").unwrap().as_str(), "x");
}

#[test]
fn root_name_accepts_the_root_separator() {
  assert_eq!(RootName::new("/").unwrap().as_str(), "/");
}

#[test]
fn root_name_accepts_a_single_leading_separator() {
  assert_eq!(RootName::new("/user").unwrap().as_str(), "/user");
  assert_eq!(RootName::new("/x").unwrap().as_str(), "/x");
  assert_eq!(RootName::new("user").unwrap().as_str(), "user");
}

#[test]
fn root_name_rejects_an_empty_name() {
  assert!(RootName::new("").is_err(), "an empty root name was accepted");
}

#[test]
fn root_name_rejects_a_separator_after_the_first_character() {
  assert!(RootName::new("/user/boom/*").is_err(), "a root name carrying a later `/` was accepted");
  assert!(RootName::new("/usr/local").is_err());
  assert!(RootName::new("us/er").is_err());
  assert!(RootName::new("//").is_err());
  assert!(RootName::new("a/").is_err());
}

#[test]
fn root_name_rejects_a_fragment_separator() {
  assert!(RootName::new("/a#b").is_err(), "a root name carrying `#` was accepted");
}

#[test]
fn root_actor_path_with_name_rejects_a_separator_after_the_first_character() {
  let error = RootActorPath::with_name(address(), "/user/boom").unwrap_err();

  assert!(error.contains("is a path separator"), "the error lost its message: {error}");
}

#[test]
fn root_actor_path_with_name_accepts_the_root_separator() {
  let root = RootActorPath::with_name(address(), "/").unwrap();

  assert_eq!(root.name(), "/");
}

#[test]
fn root_actor_path_new_uses_the_root_separator() {
  assert_eq!(RootActorPath::new(address()).name(), "/");
}

#[test]
fn with_child_rejects_a_path_separator() {
  let root = RootActorPath::new(address());

  let outcome = std::panic::catch_unwind(|| root.with_child("us/er"));

  assert!(outcome.is_err(), "a child carrying `/` was accepted");
}

#[test]
fn with_child_rejects_a_fragment_separator() {
  let root = RootActorPath::new(address());

  let outcome = std::panic::catch_unwind(|| root.with_child("us#er"));

  assert!(outcome.is_err(), "a child carrying `#` was accepted");
}

#[test]
fn with_child_rejects_an_empty_name() {
  let root = RootActorPath::new(address());

  let outcome = std::panic::catch_unwind(|| root.with_child(""));

  assert!(outcome.is_err(), "an empty child name was accepted");
}

#[test]
fn with_child_parses_an_escaped_separator_as_one_element() {
  let path = RootActorPath::new(address()).with_child("us%2Fer");

  assert_eq!(path.elements(), vec!["us%2Fer".to_string()]);
}

#[test]
fn with_child_appends_the_name() {
  let path = RootActorPath::new(address()).with_child("user").with_child("foo");

  assert_eq!(path.elements(), vec!["user".to_string(), "foo".to_string()]);
  assert_eq!(path.name(), "foo");
}

#[test]
fn a_root_frame_has_a_single_element() {
  let path = RootActorPath::new(address()).with_child("user");

  assert_eq!(path.frames().len(), 1);
  assert_eq!(path.frames()[0].as_str(), "user");
}

impl ActorPath {
  fn frames(&self) -> &[PathElement] {
    &self.elements
  }
}

#[test]
fn actor_path_from_string_keeps_going_through_element_creation() {
  let path = ActorPath::from_string("pekko://mysys/user/foo").unwrap();

  assert_eq!(path.elements(), vec!["user".to_string(), "foo".to_string()]);
}
