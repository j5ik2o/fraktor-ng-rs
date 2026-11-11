//! The child modules of a module, and whether it is a leaf.

use syn::{Item, ItemMod, Meta, visit::Visit};

use super::source_text;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Leafness {
    Leaf,
    Branch,
    /// The module's file is missing, ambiguous, or does not parse.
    Unknown,
}

/// The `mod` items of a module: its own items and the items nested in their
/// bodies, but not the items of nested modules. This is what the old lint got
/// from `hir_module_items(module).free_items()`.
pub(super) fn declared_modules(items: &[Item]) -> Vec<&ItemMod> {
    let mut collector = Collector::default();
    for item in items {
        collector.visit_item(item);
    }
    collector.modules
}

/// A module is a leaf when it declares no child module. A child that exists
/// only under `cfg(test)` does not count.
pub(super) fn leafness(items: &[Item]) -> Leafness {
    let has_child = declared_modules(items)
        .iter()
        .any(|module| !requires_test(module));
    if has_child {
        Leafness::Branch
    } else {
        Leafness::Leaf
    }
}

/// Whether a `cfg` on the module can hold only when `test` is set.
fn requires_test(module: &ItemMod) -> bool {
    module.attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && matches!(&attr.meta, Meta::List(list) if source_text::cfg_requires_test(&list.tokens.to_string()))
    })
}

#[derive(Default)]
struct Collector<'ast> {
    modules: Vec<&'ast ItemMod>,
}

impl<'ast> Visit<'ast> for Collector<'ast> {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.modules.push(node);
    }
}
