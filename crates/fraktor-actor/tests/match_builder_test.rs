// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/java/org/apache/pekko/japi/MatchBuilderTest.java
//
// The original's three `@Test` cases are kept as one test each, named after the
// case, with the original name as a comment.
//
// The original builds a `Match<Object, R>`, whose input is any object. The
// port's counterpart of `Object` is the type-erased `Message`, so the cases
// build a `PartialFunction<Message, R>`. The original API and its Rust
// counterpart:
//
// - `Match.create(Match.match(...).match(...))` is
//   `PartialFunctionBuilder::new().match_type::<P>(...)....build()`. The port has
//   one type for a built partial function, and `Match` is that type.
// - `match(Integer.class, apply)` is `match_type::<i32>(apply)`: the case is
//   defined for a message whose payload is an `i32`.
// - `match(Number.class, apply)` matches a supertype. A `Message` payload has
//   no supertype to match on, so the port's counterpart is `match_extract`
//   with the `number` extractor below, which reads any numeric payload as an
//   `f64`, the way `Number.doubleValue()` does. The extractor accepts an `i32`
//   too, so the first case's priority over the second is observed as in the
//   original, where `Integer` is also a `Number`.
// - `matchUnchecked(GenericClass.class, apply)` exists because Java erases the
//   type argument of `GenericClass<String>`. Rust keeps it, so the port's
//   counterpart is the checked `match_type::<GenericClass<String>>(apply)`.
// - `matchUnchecked(type, predicate, apply)` is
//   `match_type_if::<GenericClass<String>>(predicate, apply)`.
// - `pf.match(x)` is `apply(&Message::new(x))`, which returns
//   `Result<R, MatchError>`. The original throws `scala.MatchError` when no case
//   applies; `assertThrows(MatchError.class, ...)` is `Err(MatchError)`.
//
// The test calls the production `PartialFunction`, `PartialFunctionBuilder`,
// and `MatchError`. Until they exist the file does not compile: that compile
// error is the intended red for this node.

use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::partial_function::{MatchError, PartialFunction, PartialFunctionBuilder};

/// Reads a numeric payload as an `f64`, as `Number.doubleValue()` does for
/// the boxed numbers of Java.
fn number(message: &Message) -> Option<f64> {
  if let Some(value) = message.downcast_ref::<i32>() {
    return Some(f64::from(*value));
  }
  if let Some(value) = message.downcast_ref::<i64>() {
    return Some(*value as f64);
  }
  if let Some(value) = message.downcast_ref::<f32>() {
    return Some(f64::from(*value));
  }
  message.downcast_ref::<f64>().copied()
}

// "shouldPassBasicMatchTest"
#[test]
fn should_pass_basic_match_test() {
  let pf: PartialFunction<Message, f64> = PartialFunctionBuilder::new()
    .match_type::<i32>(|integer| f64::from(*integer) * 10.0)
    .match_extract(number, |number| number * (-10.0))
    .build();

  assert_eq!(pf.apply(&Message::new(4711_i32)), Ok(47110.0), "An integer should be multiplied by 10");
  assert_eq!(pf.apply(&Message::new(4711.0_f64)), Ok(-47110.0), "A double should be multiplied by -10");
  assert_eq!(pf.apply(&Message::new("4711")), Err(MatchError), "A string should throw a MatchError");
}

struct GenericClass<T> {
  val: T,
}

impl<T> GenericClass<T> {
  fn new(val: T) -> Self {
    Self { val }
  }
}

// "shouldHandleMatchOnGenericClass"
#[test]
fn should_handle_match_on_generic_class() {
  let pf: PartialFunction<Message, String> = PartialFunctionBuilder::new()
    .match_type::<GenericClass<String>>(|string_generic_class| string_generic_class.val.clone())
    .build();

  assert_eq!(
    pf.apply(&Message::new(GenericClass::new(String::from("A")))),
    Ok(String::from("A")),
    "String value should be extract from GenericMessage"
  );
}

// "shouldHandleMatchWithPredicateOnGenericClass"
#[test]
fn should_handle_match_with_predicate_on_generic_class() {
  let pf: PartialFunction<Message, String> = PartialFunctionBuilder::new()
    .match_type_if::<GenericClass<String>>(
      |generic_class| !generic_class.val.is_empty(),
      |string_generic_class| string_generic_class.val.clone(),
    )
    .build();

  assert_eq!(
    pf.apply(&Message::new(GenericClass::new(String::new()))),
    Err(MatchError),
    "empty GenericMessage should throw match error"
  );
}
