//! An immutable index that matches element sequences against wildcard
//! patterns.

mod wildcard_index_error;
mod wildcard_index_value;

pub use wildcard_index_error::WildcardIndexError;
pub use wildcard_index_value::WildcardIndex;
