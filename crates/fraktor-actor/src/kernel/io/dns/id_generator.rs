//! The trait that produces DNS message identifiers.

use super::IdGenerationError;

/// Produces the identifier of a DNS query.
///
/// Mirrors `org.apache.pekko.io.dns.IdGenerator`, whose `nextId(): Short`
/// returns the identifier. The method takes `&self` so a single generator may be
/// shared across threads, as the original's rustdoc promises.
pub trait IdGenerator: Send + Sync {
  /// Returns the next identifier, or [`IdGenerationError`] when the source of
  /// randomness failed.
  fn next_id(&self) -> Result<i16, IdGenerationError>;
}
