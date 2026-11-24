//! The error an unsupported ID generator policy reports.

use core::fmt;

use crate::kernel::io::dns::IdGeneratorPolicy;

/// The reason [`RandomIdGenerator::new`](super::RandomIdGenerator::new) rejected
/// a policy.
///
/// The original's enhanced double hash generator is not ported, so
/// `IdGeneratorPolicy::EnhancedDoubleHashRandom` has no generator and is
/// reported here with its policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedIdGeneratorPolicyError {
  policy: IdGeneratorPolicy,
}

impl UnsupportedIdGeneratorPolicyError {
  pub(crate) fn new(policy: IdGeneratorPolicy) -> Self {
    Self { policy }
  }

  /// Returns the rejected policy.
  pub fn policy(&self) -> IdGeneratorPolicy {
    self.policy
  }
}

impl fmt::Display for UnsupportedIdGeneratorPolicyError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "the id generator policy {:?} is not supported", self.policy)
  }
}

impl std::error::Error for UnsupportedIdGeneratorPolicyError {}
