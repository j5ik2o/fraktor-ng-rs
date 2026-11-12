// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/ActorPathSpec.scala
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
