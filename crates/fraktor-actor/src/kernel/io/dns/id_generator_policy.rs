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

impl IdGeneratorPolicy {
  /// Reads the `id-generator-policy` setting, as the original's
  /// `IdGeneratorPolicy` does, or returns `None` for an unknown text.
  ///
  /// The match is case-sensitive. An empty text names
  /// `EnhancedDoubleHashRandom`, as in the original.
  pub(crate) fn from_config_value(text: &str) -> Option<Self> {
    match text {
      | "thread-local-random" => Some(Self::ThreadLocalRandom),
      | "secure-random" => Some(Self::SecureRandom),
      | "" | "enhanced-double-hash-random" => Some(Self::EnhancedDoubleHashRandom),
      | _ => None,
    }
  }
}
