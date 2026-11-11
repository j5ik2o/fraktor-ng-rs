use std::path::Path;

use syn::{Item, visit::Visit};

use crate::{
    ast, check_context::CheckContext, paths, rule::Rule, sink::Sink, source_file::SourceFile,
};

/// Requires `use` and `mod` declarations at the top of a file or module, and
/// forbids `use` inside functions and blocks. Ported from the old
/// `use-placement-lint` (`use_mod_placement`), which already ran on syn.
///
/// Test, bench and example files are out of scope, see [`is_outside_lib_target`].
pub(crate) struct UsePlacement;

impl Rule for UsePlacement {
    fn id(&self) -> &'static str {
        "use-placement"
    }

    fn check(&self, file: &SourceFile, _cx: &CheckContext<'_>, sink: &mut Sink) {
        if is_outside_lib_target(file.path()) {
            return;
        }
        if let Some(ast) = file.ast() {
            enforce_module_items(&ast.items, sink);
        }
    }
}

/// Whether the file holds test, bench or example code, judged from its path
/// alone: anything under a `tests`, `benches` or `examples` directory, and
/// `tests.rs`, `*_test.rs`, `*_tests.rs`.
///
/// The old lint had no such exemption, but the old CI ran it on the lib target
/// only, so it never visited a file that is compiled only for tests, benches or
/// examples. The file names are the ones `tests-location` treats as test files.
/// `#[cfg(test)]` code inside any other file is still checked: the old lint
/// re-read every file it visited with syn, which does not evaluate `cfg`.
fn is_outside_lib_target(path: &Path) -> bool {
    paths::is_under_tests_dir(path)
        || paths::is_test_file_name(path)
        || path.components().any(|component| {
            let name = component.as_os_str();
            name == "benches" || name == "examples"
        })
}

fn enforce_module_items(items: &[Item], sink: &mut Sink) {
    let mut seen_non_top = false;

    for item in items {
        match classify_item(item) {
            ItemClass::UseOrMod => {
                if seen_non_top {
                    sink.report(
                        ast::start(item),
                        "`use` と `mod` 宣言はファイルまたはモジュール冒頭にまとめて配置してください",
                    );
                }
            }
            ItemClass::Ignored => {}
            ItemClass::Other => {
                seen_non_top = true;
                BlockLocalUseVisitor { sink }.visit_item(item);
            }
        }

        if let Item::Mod(module) = item
            && let Some((_, nested_items)) = &module.content
        {
            enforce_module_items(nested_items, sink);
        }
    }
}

enum ItemClass {
    /// A `use`, or a `mod` declaration without a body. Must come before `Other`.
    UseOrMod,
    /// Does not count either way: `extern crate`, a type alias, an inline module.
    Ignored,
    Other,
}

fn classify_item(item: &Item) -> ItemClass {
    match item {
        Item::Use(_) => ItemClass::UseOrMod,
        Item::Mod(module) if module.content.is_none() => ItemClass::UseOrMod,
        Item::Mod(_) | Item::ExternCrate(_) | Item::Type(_) => ItemClass::Ignored,
        _ => ItemClass::Other,
    }
}

struct BlockLocalUseVisitor<'a> {
    sink: &'a mut Sink,
}

impl<'ast> Visit<'ast> for BlockLocalUseVisitor<'_> {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.sink.report(
            ast::start(item),
            "関数やブロック内での `use` インポートは禁止です。ファイルまたはモジュール冒頭の `use` ブロックへ移動してください",
        );
    }
}
