//! The record type code a resource record holds.

use super::RecordType;

#[cfg(test)]
#[path = "record_type_code_test.rs"]
mod tests;

/// The 16-bit record type code of a resource record, known or not.
///
/// Mirrors the `recType` that `org.apache.pekko.io.dns.ResourceRecord.parse`
/// reads and that `UnknownRecord` keeps as a `Short`. A record may hold any
/// code, including one [`RecordType`] has no constant for, so this type keeps
/// the code itself and tells whether it is a registered record type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordTypeCode {
  value: u16,
}

impl RecordTypeCode {
  /// Wraps a record type code as a message holds it.
  #[must_use]
  pub const fn new(value: u16) -> Self {
    Self { value }
  }

  /// Returns the code as a message holds it.
  #[must_use]
  pub const fn value(self) -> u16 {
    self.value
  }

  /// Returns the registered record type with this code, or `None` when there
  /// is none.
  #[must_use]
  pub fn record_type(self) -> Option<RecordType> {
    RecordType::from_code(self.value)
  }
}

impl From<RecordType> for RecordTypeCode {
  fn from(record_type: RecordType) -> Self {
    Self::new(record_type.code())
  }
}
