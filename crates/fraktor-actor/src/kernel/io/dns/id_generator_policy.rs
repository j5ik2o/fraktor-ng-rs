//! The ID generator policy a DNS resolver uses.

/// Which generator produces DNS message identifiers.
///
/// Mirrors `org.apache.pekko.io.dns.IdGenerator.Policy`. `EnhancedDoubleHashRandom`
/// exists only to name the original's third configured policy; this port does not
/// implement the enhanced double hash generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdGeneratorPolicy {
  /// A non-cryptographic pseudo-random generator kept per thread.
  ///
  /// Mirrors the original `IdGenerator.Policy.ThreadLocalRandom`.
  ThreadLocalRandom,
  /// A generator drawing each identifier from the operating system's entropy.
  ///
  /// Mirrors the original `IdGenerator.Policy.SecureRandom`.
  SecureRandom,
  /// The original's enhanced double hash policy, identified but not generated.
  ///
  /// Mirrors the original `IdGenerator.Policy.EnhancedDoubleHashRandom`. No
  /// generator is ported for it, so [`crate::adaptor::RandomIdGenerator::new`]
  /// rejects it.
  EnhancedDoubleHashRandom,
}
