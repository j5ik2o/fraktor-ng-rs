use crate::{layout::Layout, workspace::Workspace};

/// What a rule can use besides the file it checks.
pub struct CheckContext<'a> {
    pub layout: Layout,
    pub workspace: &'a Workspace,
}
