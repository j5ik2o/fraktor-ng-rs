//! An identifier generator that draws DNS message ids from randomness.

use core::cell::Cell;
use core::hash::BuildHasher;
use core::hash::Hasher;

use crate::kernel::io::dns::{IdGenerationError, IdGenerator, IdGeneratorPolicy};

use super::UnsupportedIdGeneratorPolicyError;

#[cfg(test)]
#[path = "random_id_generator_test.rs"]
mod tests;

// The increment added to the per-thread state on each draw, as in the
// SplitMix-style state of Java's `ThreadLocalRandom`.
const THREAD_LOCAL_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

thread_local! {
  /// The current thread's pseudo-random state, created on first use.
  static THREAD_LOCAL_STATE: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Produces DNS message identifiers from a pseudo-random or secure source.
///
/// Mirrors the original's `IdGenerator(random, policy)`. `new` keeps the
/// requested policy, and `next_id` follows the original's
/// `(rand.nextInt(65536) + Short.MinValue).toShort` mapping.
#[derive(Debug)]
pub struct RandomIdGenerator {
  policy: IdGeneratorPolicy,
}

impl RandomIdGenerator {
  /// Creates a generator for `policy`.
  ///
  /// `ThreadLocalRandom` and `SecureRandom` are accepted.
  /// `EnhancedDoubleHashRandom` has no ported generator, so it returns
  /// [`UnsupportedIdGeneratorPolicyError`] rather than quietly generating ids
  /// with another policy.
  pub fn new(policy: IdGeneratorPolicy) -> Result<Self, UnsupportedIdGeneratorPolicyError> {
    match policy {
      | IdGeneratorPolicy::ThreadLocalRandom | IdGeneratorPolicy::SecureRandom => Ok(Self { policy }),
      | IdGeneratorPolicy::EnhancedDoubleHashRandom => Err(UnsupportedIdGeneratorPolicyError::new(policy)),
    }
  }

  /// Returns the policy this generator uses.
  pub fn policy(&self) -> IdGeneratorPolicy {
    self.policy
  }
}

impl IdGenerator for RandomIdGenerator {
  fn next_id(&self) -> Result<i16, IdGenerationError> {
    let draw = match self.policy {
      | IdGeneratorPolicy::ThreadLocalRandom => Ok(next_thread_local_draw()),
      | IdGeneratorPolicy::SecureRandom => next_secure_draw(),
      | IdGeneratorPolicy::EnhancedDoubleHashRandom => unreachable!("`new` rejects EnhancedDoubleHashRandom"),
    };
    draw.map(to_id)
  }
}

// The original maps a `nextInt(65536)` draw `x` to `x - Short.MinValue`; with
// `Short.MinValue` at -32768 that is `x - 32768`, which lands in `i16`'s range.
fn to_id(draw: u16) -> i16 {
  (i32::from(draw) - 32768) as i16
}

// One draw from this thread's SplitMix-style state, seeded on first use.
fn next_thread_local_draw() -> u16 {
  THREAD_LOCAL_STATE.with(|state| {
    let current = state.get().unwrap_or_else(seed);
    let next = current.wrapping_add(THREAD_LOCAL_GAMMA);
    state.set(Some(next));
    (mix32(next) & 0xFFFF) as u16
  })
}

// The original's `mix32` over the SplitMix output.
fn mix32(mut value: u64) -> u32 {
  value = (value ^ (value >> 33)).wrapping_mul(0x62A9_D9ED_7997_05F5);
  ((value ^ (value >> 28)).wrapping_mul(0xCB24_D0A5_C88C_35B3) >> 32) as u32
}

// A fresh per-thread seed, from the standard library's random hasher key.
fn seed() -> u64 {
  std::collections::hash_map::RandomState::new().build_hasher().finish()
}

// Two bytes from the operating system's entropy source, as the original's
// `nextInt(65536)` draw.
fn next_secure_draw() -> Result<u16, IdGenerationError> {
  let mut bytes = [0_u8; 2];
  getrandom::fill(&mut bytes).map_err(|_| IdGenerationError::EntropyUnavailable)?;
  Ok(u16::from_ne_bytes(bytes))
}
