//! analyze_tests_file: テストファイルでは内部属性の cfg(test) だけを違反にする。
#![cfg(test)]
#![ cfg( test ) ]
#![cfg(all(test, feature = "std"))]
#![cfg(not(test))]
#![allow(dead_code)]

use super::*;

mod tests {
    #[test]
    fn inline_module_is_fine_here() {}
}

#[cfg(test)]
fn helper() {}

#[test]
fn direct_test_is_fine_here() {}
