//! The seal that keeps the frequency sketch hashers inside this crate.

/// The supertrait of [`FrequencySketchHasher`](super::FrequencySketchHasher).
///
/// The trait is public so it can bound a public trait, but its module is
/// private, so code outside this crate cannot name it or implement it. It
/// takes the value type, as `FrequencySketchHasher` does, so each pair of a
/// hasher and a value type is sealed: code outside this crate can neither
/// write a new hasher nor give a hasher of this crate another value type.
/// Only the pairs this crate provides implement `FrequencySketchHasher`, as
/// only the original's own hashers implement its `sealed trait Hasher`.
pub trait HasherSeal<A: ?Sized> {}
