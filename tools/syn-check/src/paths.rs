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

    if path
        .components()
        .any(|component| component.as_os_str() == "target")
    {
        return true;
    }

    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        if component.as_os_str() == "tests" {
            if components
                .peek()
                .is_some_and(|next| next.as_os_str() == "ui")
            {
                continue;
            }
            return true;
        }
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name == "tests.rs" || name.ends_with("_test.rs") || name.ends_with("_tests.rs")
        })
}
