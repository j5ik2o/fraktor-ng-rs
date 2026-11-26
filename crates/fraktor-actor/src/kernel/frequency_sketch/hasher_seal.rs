//! The seal that keeps the frequency sketch hashers inside this crate.

/// The supertrait of [`FrequencySketchHasher`](super::FrequencySketchHasher).
///
/// The trait is public so it can bound a public trait, but its module is
/// private, so code outside this crate cannot name it or implement it. Only
/// the hashers this crate provides implement `FrequencySketchHasher`, as only
/// the original's own hashers implement its `sealed trait Hasher`.
pub trait HasherSeal {}
