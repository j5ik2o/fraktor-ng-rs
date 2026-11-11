pub struct Public;

pub(crate) struct CrateOnly;

pub(super) enum ParentOnly {}

pub(in crate::kernel) trait Scoped {}

pub union NotCounted {
    value: u32,
}

pub type AliasNotCounted = Public;
