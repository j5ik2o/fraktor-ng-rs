use crate::layout::Layout;

/// How to run the checker.
#[derive(Clone, Debug)]
pub struct Options {
    pub layout: Layout,
    /// The rule ids to run. `None` runs every rule that is enabled by default.
    pub rules: Option<Vec<String>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            layout: Layout::Ng,
            rules: None,
        }
    }
}
