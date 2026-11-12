//! Extracts the path elements of a relative actor path.

use crate::kernel::address::uri_parts::RelativeUriParts;

/// Entry point for splitting a relative actor path into its name elements.
#[derive(Clone, Copy, Debug, Default)]
pub struct RelativeActorPath;

impl RelativeActorPath {
    /// Splits `path` into its name elements, or returns `None` when `path`
    /// is not a relative actor path.
    ///
    /// The raw path is split on `/` without dropping empty elements and
    /// without percent-decoding. When a fragment is present it is appended
    /// to the last element after `#`.
    pub fn try_apply(path: &str) -> Option<Vec<String>> {
        let parts = RelativeUriParts::parse(path)?;
        let mut elements: Vec<String> = parts.raw_path().split('/').map(str::to_string).collect();
        if let Some(fragment) = parts.raw_fragment() {
            let last = elements.len() - 1;
            elements[last].push('#');
            elements[last].push_str(fragment);
        }
        Some(elements)
    }
}
