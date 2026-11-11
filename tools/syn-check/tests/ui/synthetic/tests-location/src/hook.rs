//! is_sibling_test_module_hook と is_tests_module_declaration の例外。

#[cfg(test)]
#[path = "hook_test.rs"]
mod tests;

#[cfg(test)] #[path="hook_test.rs"] pub mod tests;

#[cfg(test)]
#[ path = "nested/hook_test.rs" ]
pub(crate) mod tests;

#[cfg(test)]
#[path = "hook_test.rs"]
pub(in crate::a) mod tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) mod tests;

#[path = "hook_test.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests {}
