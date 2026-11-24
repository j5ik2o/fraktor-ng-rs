//! The resolved positive and negative cache policies.

use super::Ttl;

/// How long a record is cached.
///
/// Mirrors `org.apache.pekko.io.dns.CachePolicy`, which is `Never`, `Forever`,
/// or a [`Ttl`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CachePolicy {
  /// Do not cache.
  Never,
  /// Cache forever.
  Forever,
  /// Cache for the given TTL.
  Ttl(Ttl),
}
