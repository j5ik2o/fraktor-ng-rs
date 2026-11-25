// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 9dfca01566 #3348; the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/io/dns/IdGeneratorSpec.scala
//
// The original's two cases under `"IdGenerator" must` are kept as one test
// each, named after the case, with the original name as a comment. Each case
// builds an `IdGenerator` for one policy and calls `nextId()` twice, asserting
// the first id is at most `Short.MaxValue` and the second is at least
// `Short.MinValue`. Those bounds never fail for an `i16`; the cases only check
// that the generator returns a value of the range's type.
//
// The original's `IdGenerator(policy)` is the port's
// `RandomIdGenerator::new(policy)`, and `nextId(): Short` is
// `next_id() -> Result<i16, IdGenerationError>`. The ids are compared as `i32`
// so that the bounds check does not become an `absurd_extreme_comparisons`
// lint, which would deny the comparison between an `i16` and `i16::MAX`.
//
// The port's `IdGeneratorPolicy` lives in `fraktor_actor::kernel::io::dns` and
// `RandomIdGenerator` in `fraktor_actor::adaptor`. Until they exist the file
// does not compile: that compile error is the intended red for this node.

use fraktor_actor::adaptor::RandomIdGenerator;
use fraktor_actor::kernel::io::dns::IdGenerator;
use fraktor_actor::kernel::io::dns::IdGeneratorPolicy;

// "provide a thread-local-random"
#[test]
fn provide_a_thread_local_random() {
  let generator = RandomIdGenerator::new(IdGeneratorPolicy::ThreadLocalRandom).expect("thread-local-random generator");
  let first: i16 = generator.next_id().expect("first id");
  assert!(i32::from(first) <= i32::from(i16::MAX));
  let second: i16 = generator.next_id().expect("second id");
  assert!(i32::from(second) >= i32::from(i16::MIN));
}

// "provide a secure-random"
#[test]
fn provide_a_secure_random() {
  let generator = RandomIdGenerator::new(IdGeneratorPolicy::SecureRandom).expect("secure-random generator");
  let first: i16 = generator.next_id().expect("first id");
  assert!(i32::from(first) <= i32::from(i16::MAX));
  let second: i16 = generator.next_id().expect("second id");
  assert!(i32::from(second) >= i32::from(i16::MIN));
}
