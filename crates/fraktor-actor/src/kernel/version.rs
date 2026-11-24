//! Version comparison and compatibility checking.

mod fraktor_version;
mod release_version;
mod unsupported_fraktor_version;
mod version_parse_error;
mod version_value;

pub use fraktor_version::FraktorVersion;
pub use unsupported_fraktor_version::UnsupportedFraktorVersion;
pub use version_parse_error::VersionParseError;
pub use version_value::Version;
