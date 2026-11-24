//! Unit tests for [`RandomIdGenerator`](super::RandomIdGenerator).

use alloc::string::ToString;
use std::collections::HashSet;
use std::sync::Arc;
use std::thread;

use crate::kernel::io::dns::{IdGenerationError, IdGenerator, IdGeneratorPolicy};

use super::{RandomIdGenerator, UnsupportedIdGeneratorPolicyError};

fn generator(policy: IdGeneratorPolicy) -> RandomIdGenerator {
  RandomIdGenerator::new(policy).expect("a supported policy")
}

// The supported policies are kept; reading `policy()` back proves `new` stored
// the requested policy rather than falling back to another one.
#[test]
fn thread_local_random_is_kept() {
  assert_eq!(generator(IdGeneratorPolicy::ThreadLocalRandom).policy(), IdGeneratorPolicy::ThreadLocalRandom);
}

#[test]
fn secure_random_is_kept() {
  assert_eq!(generator(IdGeneratorPolicy::SecureRandom).policy(), IdGeneratorPolicy::SecureRandom);
}

// The original's enhanced double hash generator is not ported. Rather than
// silently substituting another policy, `new` rejects it and the error names
// the rejected policy.
#[test]
fn the_enhanced_double_hash_policy_is_rejected_with_its_name() {
  let error: UnsupportedIdGeneratorPolicyError =
    RandomIdGenerator::new(IdGeneratorPolicy::EnhancedDoubleHashRandom).expect_err("an unsupported policy");
  assert_eq!(error.policy(), IdGeneratorPolicy::EnhancedDoubleHashRandom);
  assert!(!error.to_string().is_empty());
}

#[test]
fn the_thread_local_generator_produces_ids() {
  let generator = generator(IdGeneratorPolicy::ThreadLocalRandom);
  assert!(generator.next_id().is_ok());
  assert!(generator.next_id().is_ok());
}

#[test]
fn the_secure_generator_produces_ids() {
  let generator = generator(IdGeneratorPolicy::SecureRandom);
  assert!(generator.next_id().is_ok());
  assert!(generator.next_id().is_ok());
}

// `ThreadLocalRandom` keeps its state per thread, so a generator shared across
// threads must still produce ids from each thread's own state.
#[test]
fn the_thread_local_generator_produces_ids_on_another_thread() {
  let generator = Arc::new(generator(IdGeneratorPolicy::ThreadLocalRandom));
  let handle = {
    let generator = Arc::clone(&generator);
    thread::spawn(move || (generator.next_id().is_ok(), generator.next_id().is_ok()))
  };
  let (first, second) = handle.join().expect("the worker thread");
  assert!(first);
  assert!(second);
}

// `next_id(&self)` must be callable from several threads at once, which the
// original's rustdoc promises.
#[test]
fn the_generator_is_send_and_sync() {
  fn assert_send_and_sync<T: Send + Sync>() {}
  assert_send_and_sync::<RandomIdGenerator>();
}

// The original maps a `nextInt(65536)` draw `x` to `x - Short.MinValue`. These
// three draws are the range ends and the point that maps to zero; an `as i16`
// cast without the offset, or an offset in the wrong direction, breaks them.
#[test]
fn a_draw_of_zero_maps_to_the_minimum() {
  assert_eq!(super::to_id(0), i16::MIN);
}

#[test]
fn a_draw_of_32768_maps_to_zero() {
  assert_eq!(super::to_id(32768), 0);
}

#[test]
fn a_draw_of_65535_maps_to_the_maximum() {
  assert_eq!(super::to_id(65535), i16::MAX);
}

// Every `u16` draw must map into `i16` without panicking; the cast is the only
// step, so this guards against a subtraction done in the narrower type.
#[test]
fn every_draw_maps_into_the_short_range() {
  for draw in [1_u16, 1000, 40000, 65534] {
    let id = super::to_id(draw);
    assert_eq!(i32::from(id), i32::from(draw) - 32768);
  }
}

// The secure path draws from the OS and must not return the same value every
// time; a constant would be a failure to read the entropy source.
#[test]
fn the_secure_generator_varies_across_calls() {
  let generator = generator(IdGeneratorPolicy::SecureRandom);
  let mut seen = HashSet::new();
  for _ in 0..64 {
    seen.insert(generator.next_id().expect("a secure id"));
  }
  assert!(seen.len() > 1);
}

// The entropy failure must be reported rather than swallowed or replaced by
// another policy. The branch cannot be reached in a test, so this only pins the
// error value the implementation returns.
#[test]
fn the_entropy_error_names_the_failure() {
  assert!(!IdGenerationError::EntropyUnavailable.to_string().is_empty());
}
