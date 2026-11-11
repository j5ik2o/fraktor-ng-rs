//! find_cfg_test_items は cfg(test) 属性を一字一句の一致でだけ拾う。複合 cfg は違反にならない。
#![cfg(test)]

#[cfg(all(test, feature = "std"))]
fn with_std() {}

#[cfg(any(test, feature = "testing"))]
fn testing() {}

#[cfg(not(test))]
fn production() {}

#[cfg_attr(test, derive(Debug))]
struct Traced;

#[cfg( test )]
fn spaced() {}

#[cfg(all(test, unix))]
#[path = "composite_cfg_test.rs"]
mod tests;
