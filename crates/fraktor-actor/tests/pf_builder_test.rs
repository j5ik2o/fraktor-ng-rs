// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/java/org/apache/pekko/japi/pf/PFBuilderTest.java
//
// The original's one `@Test` case is kept as one test, named after the case,
// with the original name as a comment.
//
// The original API and its Rust counterpart:
//
// - `PFBuilder.<String, Integer>create()` is
//   `PartialFunctionBuilder::<String, i32>::new()`. The input type stays
//   `String`, so the closure given to `match_any` gets a `&String` without an
//   annotation: that inference is what the original case checks.
// - `matchEquals("hello", s -> 1)` is `match_equals(String::from("hello"), |_| 1)`.
// - `matchAny(s -> Integer.valueOf(s))` is `match_any(|s| ...)`, which parses
//   the string. The original's `Integer.valueOf` throws on a string that is not
//   a number; the port panics the same way, and the case only applies it to `42`.
// - `build()` is `build()`, which returns a `PartialFunction<String, i32>`.
// - `pf.isDefinedAt(x)` is `is_defined_at(&x)`.
// - `pf.apply(x)` is `apply(&x)`, which returns `Result<i32, MatchError>`; the
//   original returns the value or throws `scala.MatchError`.
//
// The test calls the production `PartialFunction` and `PartialFunctionBuilder`.
// Until they exist the file does not compile: that compile error is the
// intended red for this node.

use fraktor_actor::kernel::partial_function::{PartialFunction, PartialFunctionBuilder};

// "pfbuilder_matchAny_should_infer_declared_input_type_for_lambda"
#[test]
fn pfbuilder_match_any_should_infer_declared_input_type_for_lambda() {
  let pf: PartialFunction<String, i32> = PartialFunctionBuilder::<String, i32>::new()
    .match_equals(String::from("hello"), |_| 1)
    .match_any(|s| s.parse::<i32>().expect("the original's Integer.valueOf throws on a non-number"))
    .build();

  assert!(pf.is_defined_at(&String::from("hello")));
  assert!(pf.is_defined_at(&String::from("42")));
  assert_eq!(pf.apply(&String::from("42")), Ok(42));
}
