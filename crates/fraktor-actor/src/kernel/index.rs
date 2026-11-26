//! A concurrent multi-map from keys to comparator-ordered sets of values.

mod index_value;

pub use index_value::{ConcurrentMultiMap, Index};
