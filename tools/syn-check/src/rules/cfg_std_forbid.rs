use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use syn::{
    AttrStyle, Attribute, Expr, Item, ItemUse, Lit, Meta, MetaList, Token, UseTree,
    punctuated::Punctuated,
    visit::{self, Visit},
};

use crate::{
    ast, check_context::CheckContext, layout::Layout, paths, position::Position, rule::Rule,
    sink::Sink, source_file::SourceFile, workspace::Workspace,
};

/// Forbids `#[cfg(feature = "std")]` and `use std::...` in platform-independent
/// code. Ported from the old `cfg-std-forbid-lint` (`cfg_std_forbid`), which
/// already ran on syn.
pub(crate) struct CfgStdForbid;

impl Rule for CfgStdForbid {
    fn id(&self) -> &'static str {
        "cfg-std-forbid"
    }

    fn check(&self, file: &SourceFile, cx: &CheckContext<'_>, sink: &mut Sink) {
        let Some(ast) = file.ast() else {
            return;
        };
        let in_scope = match cx.layout {
            Layout::Legacy => lint_enabled_for(file, cx.workspace),
            Layout::Ng => paths::is_ng_module_source(file.path(), "kernel"),
        };
        if !in_scope || declared_only_for_tests(file.path(), cx.workspace) {
            return;
        }

        let mut visitor = Visitor {
            findings: Vec::new(),
            allow_std: ast.attrs.iter().any(attr_is_cfg_test),
        };
        visitor.visit_file(ast);
        for (at, kind) in visitor.findings {
            sink.report(at, kind.message(cx.layout));
        }
    }
}

struct Visitor {
    findings: Vec<(Position, Violation)>,
    /// Inside an item, or after an attribute, that `cfg(test)` guards.
    allow_std: bool,
}

impl<'ast> Visit<'ast> for Visitor {
    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        if !self.allow_std && !attr_is_cfg_test(attr) && is_forbidden_cfg(attr) {
            self.findings.push((ast::start(attr), Violation::CfgStd));
        }
    }

    fn visit_item(&mut self, item: &'ast Item) {
        let prev = self.allow_std;
        self.allow_std = prev || ast::item_attrs(item).iter().any(attr_is_cfg_test);
        if !self.allow_std
            && let Item::Use(item_use) = item
            && use_tree_contains_std(item_use)
        {
            self.findings
                .push((ast::start(item_use), Violation::UseStd));
        }
        visit::visit_item(self, item);
        self.allow_std = prev;
    }
}

enum Violation {
    CfgStd,
    UseStd,
}

impl Violation {
    fn message(&self, layout: Layout) -> String {
        let destination = match layout {
            Layout::Legacy => "std 対応クレート",
            Layout::Ng => "adaptor モジュール",
        };
        match self {
            Self::CfgStd => format!(
                "`#[cfg(feature = \"std\")]` の使用が検出されました。std に依存するコードは{destination}へ移してください"
            ),
            Self::UseStd => format!(
                "`use std::...` の使用が検出されました。std のアイテムに依存するコードは{destination}へ移すか、`core` や `alloc` の代替を使ってください"
            ),
        }
    }
}

/// The old lint was `Allow` by default and ran where a crate turned it on.
/// This reads `#![deny]`, `#![warn]`, or `#![forbid]` from the file itself and
/// from the root of the crate that contains it.
fn lint_enabled_for(file: &SourceFile, workspace: &Workspace) -> bool {
    let enables = |attrs: &[Attribute]| attrs.iter().any(attr_enables_lint);
    if file.ast().is_some_and(|ast| enables(&ast.attrs)) {
        return true;
    }
    let Some(src) = file
        .path()
        .ancestors()
        .skip(1)
        .find(|dir| dir.file_name().is_some_and(|name| name == "src"))
    else {
        return false;
    };
    ["lib.rs", "main.rs"].iter().any(|root| {
        workspace
            .source(&src.join(root))
            .is_some_and(|root| root.ast().is_some_and(|ast| enables(&ast.attrs)))
    })
}

fn attr_enables_lint(attr: &Attribute) -> bool {
    let is_level = ["deny", "warn", "forbid"]
        .iter()
        .any(|level| attr.path().is_ident(level));
    if !matches!(attr.style, AttrStyle::Inner(_)) || !is_level {
        return false;
    }
    match &attr.meta {
        Meta::List(list) => parse_meta_arguments(list)
            .iter()
            .any(|meta| matches!(meta, Meta::Path(path) if path.is_ident("cfg_std_forbid"))),
        _ => false,
    }
}

/// Whether the module in this file, or one of its ancestors, is declared under
/// `#[cfg(test)]`.
///
/// rustc did not compile such a file into the old lint's view unless tests were
/// built, and a `cfg(test)` inline module was exempt. The exemption is carried
/// across the file boundary here.
fn declared_only_for_tests(path: &Path, workspace: &Workspace) -> bool {
    let mut current = path.to_path_buf();
    while let Some((declaring_file, under_cfg_test)) = declaration_of(&current, workspace) {
        if under_cfg_test {
            return true;
        }
        current = declaring_file;
    }
    false
}

/// The file that declares the module in `path`, and whether that declaration
/// is under `cfg(test)`. `None` for a crate root or a file nothing declares.
fn declaration_of(path: &Path, workspace: &Workspace) -> Option<(PathBuf, bool)> {
    if let Some(hook) = sibling_test_hook(path, workspace) {
        return Some(hook);
    }
    let (name, children_dir) = module_of(path)?;
    let parent = parent_module_file(&children_dir, workspace)?;
    let declaration = parent.ast()?.items.iter().find_map(|item| match item {
        Item::Mod(module) if module.content.is_none() && module.ident == name => Some(module),
        _ => None,
    })?;
    Some((
        parent.path().to_path_buf(),
        declaration.attrs.iter().any(attr_is_cfg_test),
    ))
}

/// The declaration `#[path = "<module>_test.rs"] mod tests;` in the sibling
/// `<module>.rs`. This is the form the `tests-location` rule asks for, so a
/// test file is usually declared this way and not by its own name.
fn sibling_test_hook(path: &Path, workspace: &Workspace) -> Option<(PathBuf, bool)> {
    let file_name = path.file_name()?.to_str()?;
    let module = file_name.strip_suffix("_test.rs")?;
    let sibling = path.with_file_name(format!("{module}.rs"));
    let source = workspace.source(&sibling)?;
    let declaration = source.ast()?.items.iter().find_map(|item| match item {
        Item::Mod(module)
            if module.content.is_none()
                && path_attribute(&module.attrs).as_deref() == Some(file_name) =>
        {
            Some(module)
        }
        _ => None,
    })?;
    Some((sibling, declaration.attrs.iter().any(attr_is_cfg_test)))
}

fn path_attribute(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        Meta::NameValue(name_value) if name_value.path.is_ident("path") => {
            match &name_value.value {
                Expr::Lit(literal) => match &literal.lit {
                    Lit::Str(value) => Some(value.value()),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    })
}

/// The module name of a file, and the directory its parent module's children live in.
/// `None` for a crate root.
fn module_of(path: &Path) -> Option<(String, PathBuf)> {
    let dir = path.parent()?;
    match path.file_name()?.to_str()? {
        "lib.rs" | "main.rs" => None,
        "mod.rs" => Some((
            dir.file_name()?.to_str()?.to_string(),
            dir.parent()?.to_path_buf(),
        )),
        _ => Some((path.file_stem()?.to_str()?.to_string(), dir.to_path_buf())),
    }
}

fn parent_module_file(children_dir: &Path, workspace: &Workspace) -> Option<Rc<SourceFile>> {
    let mut candidates = vec![
        children_dir.with_extension("rs"),
        children_dir.join("mod.rs"),
    ];
    if children_dir.file_name().is_some_and(|name| name == "src") {
        candidates.extend([children_dir.join("lib.rs"), children_dir.join("main.rs")]);
    }
    candidates
        .iter()
        .find_map(|candidate| workspace.source(candidate))
}

fn attr_is_cfg_test(attr: &Attribute) -> bool {
    if !attr.path().is_ident("cfg") {
        return false;
    }
    match &attr.meta {
        Meta::List(list) => parse_meta_arguments(list).iter().any(meta_requires_test),
        _ => false,
    }
}

fn meta_requires_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") => {
            let args = parse_meta_arguments(list);
            args.iter().any(meta_requires_test) && !args.iter().any(meta_allows_without_test)
        }
        _ => false,
    }
}

fn meta_allows_without_test(meta: &Meta) -> bool {
    match meta {
        Meta::List(list) if list.path.is_ident("not") || list.path.is_ident("any") => true,
        Meta::List(list) => parse_meta_arguments(list)
            .iter()
            .any(meta_allows_without_test),
        _ => false,
    }
}

fn is_forbidden_cfg(attr: &Attribute) -> bool {
    if !attr.path().is_ident("cfg") {
        return false;
    }
    match &attr.meta {
        Meta::List(list) => parse_meta_arguments(list).iter().any(contains_feature_std),
        _ => false,
    }
}

fn contains_feature_std(meta: &Meta) -> bool {
    match meta {
        Meta::NameValue(name_value) => {
            name_value.path.is_ident("feature") && expr_is_std(&name_value.value)
        }
        Meta::List(list) => contains_feature_std_list(list),
        Meta::Path(_) => false,
    }
}

fn contains_feature_std_list(list: &MetaList) -> bool {
    if list.path.is_ident("not") {
        return false;
    }
    let args = parse_meta_arguments(list);
    if list.path.is_ident("feature") {
        args.iter().any(|item| match item {
            Meta::Path(path) => path.is_ident("std"),
            Meta::NameValue(name_value) => expr_is_std(&name_value.value),
            Meta::List(inner) => contains_feature_std_list(inner),
        })
    } else {
        args.iter().any(contains_feature_std)
    }
}

fn expr_is_std(expr: &Expr) -> bool {
    matches!(expr, Expr::Lit(literal) if matches!(&literal.lit, Lit::Str(value) if value.value() == "std"))
}

fn parse_meta_arguments(list: &MetaList) -> Vec<Meta> {
    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map(|arguments| arguments.into_iter().collect())
        .unwrap_or_default()
}

fn use_tree_contains_std(item_use: &ItemUse) -> bool {
    fn tree_contains(tree: &UseTree) -> bool {
        match tree {
            UseTree::Path(path) => path.ident == "std" || tree_contains(&path.tree),
            UseTree::Name(name) => name.ident == "std",
            UseTree::Group(group) => group.items.iter().any(tree_contains),
            UseTree::Rename(rename) => rename.ident == "std",
            UseTree::Glob(_) => false,
        }
    }
    tree_contains(&item_use.tree)
}
