//! The check that a library's required Fraktor version is satisfied.

use super::{release_version::ReleaseVersion, unsupported_fraktor_version::UnsupportedFraktorVersion};

#[cfg(test)]
#[path = "fraktor_version_test.rs"]
mod tests;

/// Checks a library's required Fraktor version against the current one.
///
/// Mirrors `org.apache.pekko.PekkoVersion`. The check is independent of
/// [`Version`](super::Version): it compares the major, minor, patch, and
/// milestone parts of the text directly instead of parsing a [`Version`].
pub struct FraktorVersion;

impl FraktorVersion {
  /// Returns `Ok(())` when `current_version` satisfies `required_version`, or
  /// an [`UnsupportedFraktorVersion`] naming the mismatch.
  ///
  /// Equal strings, and versions that do not match
  /// `major.minor.patch[-M/RCn]`, are accepted as-is, mirroring the original's
  /// "SNAPSHOT or unknown - you're on your own" rule. Unlike the original,
  /// which throws `NumberFormatException` on a number that overflows `Int`,
  /// the numbers are compared as digit strings and never overflow.
  pub fn require(
    library_name: &str,
    required_version: &str,
    current_version: &str,
  ) -> Result<(), UnsupportedFraktorVersion> {
    if required_version == current_version {
      return Ok(());
    }
    let parsed = (ReleaseVersion::parse(required_version), ReleaseVersion::parse(current_version));
    let (Some(required), Some(current)) = parsed else {
      return Ok(());
    };
    if required.satisfies(&current) {
      Ok(())
    } else {
      Err(UnsupportedFraktorVersion::new(library_name, required_version, current_version))
    }
  }
}
