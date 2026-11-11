use core::time::Duration;
use alloc::vec::Vec;
use std;
use ::std::thread;
use std as platform;
use crate::util::{self, std::Shim};

#[cfg(any(test, feature = "std"))]
fn test_or_std() {}

#[cfg(all(test, not(feature = "std")))]
fn test_without_std() {}

#[cfg(not(feature = "std"))]
fn without_std() {}

#[cfg(feature = "alloc")]
fn alloc_only() {}

pub struct Holder {
  #[cfg(feature = "std")]
  field: u8,
}

fn body() {
  #[cfg(feature = "std")]
  use std::sync::Mutex;

  let _ = std::time::Instant::now();
  let _: std::sync::Arc<u8>;
}

impl Holder {
  #[cfg(test)]
  fn test_method() {
    use std::vec::Vec;
  }
}

#[cfg(test)]
mod inline_tests {
  use std::time::Instant;

  #[cfg(feature = "std")]
  fn std_test() {}
}
