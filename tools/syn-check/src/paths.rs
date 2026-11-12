//! Path predicates shared by more than one rule.

use std::path::Path;

/// Whether the old `type-per-file` and `ambiguous-suffix` lints skipped the file.
///
/// They skipped build output and test code: anything under `target`, anything
/// under a `tests` directory, and `tests.rs`, `*_test.rs`, `*_tests.rs`. A
/// `tests` directory directly followed by `ui` does not count, so that the
/// lints still ran on their own UI fixtures.
pub(crate) fn is_test_or_build_output(path: &Path) -> bool {
  if path.extension().is_none_or(|extension| extension != "rs") {
    return true;
  }

  if path.components().any(|component| component.as_os_str() == "target") {
    return true;
  }

  is_under_tests_dir(path) || is_test_file_name(path)
}

/// Whether the file name marks a test file: `tests.rs`, `*_test.rs`, `*_tests.rs`.
pub(crate) fn is_test_file_name(path: &Path) -> bool {
  path
    .file_name()
    .and_then(|name| name.to_str())
    .is_some_and(|name| name == "tests.rs" || name.ends_with("_test.rs") || name.ends_with("_tests.rs"))
}

/// Whether a `tests` directory is on the path.
///
/// A `tests` directory directly followed by `ui` does not count, so that the
/// rules still run on their own UI fixtures.
pub(crate) fn is_under_tests_dir(path: &Path) -> bool {
  let mut components = path.components().peekable();
  while let Some(component) = components.next() {
    if component.as_os_str() == "tests" && components.peek().is_none_or(|next| next.as_os_str() != "ui") {
      return true;
    }
  }
  false
}

/// Whether `path` is `crates/<crate>/src/<module>.rs` or under `crates/<crate>/src/<module>/`.
///
/// In fraktor-ng-rs a crate keeps its platform-independent code in the `kernel`
/// module and its std implementation in the `adaptor` module.
pub(crate) fn is_ng_module_source(path: &Path, module: &str) -> bool {
  let components: Vec<&str> = path.components().filter_map(|component| component.as_os_str().to_str()).collect();
  let file = format!("{module}.rs");
  components.windows(4).enumerate().any(|(index, window)| {
    let is_last = index + 4 == components.len();
    window[0] == "crates" && window[2] == "src" && ((window[3] == file && is_last) || (window[3] == module && !is_last))
  })
}
