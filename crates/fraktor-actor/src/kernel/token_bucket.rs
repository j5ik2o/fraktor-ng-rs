//! A token bucket that reports how long an element must wait.

mod token_bucket_error;
mod token_bucket_value;

pub use token_bucket_error::TokenBucketError;
pub use token_bucket_value::TokenBucket;
