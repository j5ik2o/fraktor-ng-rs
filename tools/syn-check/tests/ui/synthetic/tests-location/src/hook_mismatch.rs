//! is_sibling_test_module_hook に当てはまらない hook は、cfg(test) のアイテムとして違反になる。
#[cfg(test)]
#[path = "hook_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hook_test.rs"]
mod hook_tests;

#[cfg(test)]
#[path = "hook_test.rs"]
mod tests ;

#[cfg(test)]
#[allow(dead_code)]
#[path = "hook_test.rs"]
mod tests;

#[cfg(test)]
#[path = "hook_test.rs"]
pub(crate) mod tests_util;
