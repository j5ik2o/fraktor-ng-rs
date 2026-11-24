//! Unit tests for [`IdGenerationError`](super::IdGenerationError).

use super::IdGenerationError;

#[test]
fn the_display_text_names_the_failure() {
  assert!(!IdGenerationError::EntropyUnavailable.to_string().is_empty());
}

// The error is reported to callers that box `core::error::Error`, so the
// implementation must expose the trait the port promises.
#[test]
fn the_error_implements_core_error() {
  fn as_core_error<'a>(error: &'a (dyn core::error::Error + 'static)) -> &'a (dyn core::error::Error + 'static) {
    error
  }
  let _ = as_core_error(&IdGenerationError::EntropyUnavailable);
}
