// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/java/org/apache/pekko/actor/setup/ActorSystemSetupTest.java
//
// The original's one `@Test` case is kept as one test, named after the case,
// with the original name as a comment.
//
// The original API and its Rust counterpart:
//
// - `static class JavaSetup extends Setup` with a public `name` field is a
//   struct with a `name` field and an `impl Setup`. The Java class does not
//   override `equals`, so it derives nothing.
// - `ActorSystemSetup.create()` is `ActorSystemSetup::empty()`.
// - `withSetup(s)` is `with_setup(s)`.
// - `get(JavaSetup.class)`, which returns `Optional<JavaSetup>`, is
//   `get::<JavaSetup>()`, which returns `Option<&JavaSetup>`.
// - `assertEquals(javaSetting, result.get())` compares by identity, because
//   `JavaSetup` keeps `Object.equals`. The setting moves into the setup, so the
//   port cannot compare it with the variable it moved out of. It takes the
//   address of the setting's own heap data, the bytes of `name`, before the
//   move, and checks that the value it gets back owns that same data: a moved
//   value keeps its heap data, while a copy or a new `JavaSetup` with an equal
//   name would own different bytes. It also checks the name, as before.
//
// The test calls the production `Setup` and `ActorSystemSetup`. Until they
// exist the file does not compile: that compile error is the intended red for
// this node.

use fraktor_actor::kernel::setup::{ActorSystemSetup, Setup};

struct JavaSetup {
  name: String,
}

impl JavaSetup {
  fn new(name: &str) -> Self {
    Self { name: name.to_string() }
  }
}

impl Setup for JavaSetup {}

// "apiMustBeUsableFromJava"
#[test]
fn api_must_be_usable_from_java() {
  let java_setting = JavaSetup::new("Jasmine Rice");
  let registered_name: *const u8 = java_setting.name.as_ptr();
  let setups = ActorSystemSetup::empty().with_setup(java_setting);
  let result: Option<&JavaSetup> = setups.get::<JavaSetup>();

  assert!(result.is_some());
  assert_eq!(result.map(|setup| setup.name.as_str()), Some("Jasmine Rice"));
  assert!(result.is_some_and(|setup| std::ptr::eq(setup.name.as_ptr(), registered_name)));
}
