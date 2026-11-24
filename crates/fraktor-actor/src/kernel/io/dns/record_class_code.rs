//! The record class code a resource record holds.

use super::RecordClass;

#[cfg(test)]
#[path = "record_class_code_test.rs"]
mod tests;

/// The 16-bit record class code of a resource record, known or not.
///
/// Mirrors the `recClass` that `org.apache.pekko.io.dns.ResourceRecord.parse`
/// reads and that `UnknownRecord` keeps as a `Short`. A record may hold any
/// code, including one [`RecordClass`] has no constant for, so this type keeps
/// the code itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordClassCode {
  value: u16,
}

impl RecordClassCode {
  /// Wraps a record class code as a message holds it.
  #[must_use]
  pub const fn new(value: u16) -> Self {
    Self { value }
  }

  /// Returns the code as a message holds it.
  #[must_use]
  pub const fn value(self) -> u16 {
    self.value
  }
}

impl From<RecordClass> for RecordClassCode {
  fn from(record_class: RecordClass) -> Self {
    Self::new(record_class.code())
  }
}
