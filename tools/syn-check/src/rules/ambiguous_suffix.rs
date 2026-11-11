use std::path::Path;

use proc_macro2::Ident;
use syn::{
    BareFnArg, Field, FnArg, ForeignItemFn, GenericParam, ImplItemConst, ImplItemFn, ImplItemType,
    Item, PatIdent, Signature, TraitItemConst, TraitItemFn, TraitItemType, Variant,
    ext::IdentExt,
    visit::{self, Visit},
};

use crate::{
    ast, check_context::CheckContext, position::Position, rule::Rule, sink::Sink,
    source_file::SourceFile,
};

/// Flags names that end in a vague suffix such as `Manager` or `Service`.
/// Ported from the old `ambiguous-suffix-lint` (`ambiguous_suffix`).
pub(crate) struct AmbiguousSuffix;

/// A forbidden suffix in its CamelCase and snake_case spellings, and what to use instead.
struct Forbidden {
    camel: &'static str,
    snake: &'static str,
    alternatives: &'static str,
}

const FORBIDDEN: &[Forbidden] = &[
    Forbidden {
        camel: "Manager",
        snake: "manager",
        alternatives: "Registry, Coordinator, Dispatcher, Controller",
    },
    Forbidden {
        camel: "Util",
        snake: "util",
        alternatives: "具体的な動詞を含む名前 (例: FormatHelper → DateFormatter)",
    },
    Forbidden {
        camel: "Facade",
        snake: "facade",
        alternatives: "Gateway, Adapter, Bridge",
    },
    Forbidden {
        camel: "Service",
        snake: "service",
        alternatives: "Executor, Scheduler, Evaluator, Repository, Policy",
    },
    Forbidden {
        camel: "Runtime",
        snake: "runtime",
        alternatives: "Executor, Scheduler, EventLoop, Environment",
    },
    Forbidden {
        camel: "Engine",
        snake: "engine",
        alternatives: "Executor, Evaluator, Processor, Pipeline",
    },
];

/// How a name is compared with the suffix.
#[derive(Clone, Copy)]
enum NameStyle {
    /// Types, variants, associated types, type parameters: `ends_with` on the CamelCase suffix.
    Camel,
    /// Everything else: lower-cased, with the suffix as a `_`-delimited last word.
    Delimited,
}

impl Rule for AmbiguousSuffix {
    fn id(&self) -> &'static str {
        "ambiguous-suffix"
    }

    fn check(&self, file: &SourceFile, _cx: &CheckContext<'_>, sink: &mut Sink) {
        if should_ignore(file.path()) {
            return;
        }
        let Some(ast) = file.ast() else {
            return;
        };
        check_file_name(file, ast, sink);
        Checker { sink }.visit_file(ast);
    }
}

/// 旧 lint は、そのファイルで最初に検査したアイテムの位置でファイル名を報告した。
/// マクロ呼び出しは展開後のアイテムになり、展開されたアイテムは検査から外れるので、
/// 最初の「マクロ呼び出しでないアイテム」がそれに当たる。
fn check_file_name(file: &SourceFile, ast: &syn::File, sink: &mut Sink) {
    let Some(first) = ast
        .items
        .iter()
        .find(|item| !matches!(item, Item::Macro(item) if item.ident.is_none()))
    else {
        return;
    };
    let Some(stem) = file.path().file_stem().and_then(|stem| stem.to_str()) else {
        return;
    };
    // ファイル名は小文字化しない。`Runtime.rs` は対象外で、`my_runtime.rs` は対象になる。
    if let Some(forbidden) = FORBIDDEN
        .iter()
        .find(|forbidden| snake_has_suffix(stem, forbidden.snake))
    {
        report(sink, ast::decl_start(first), stem, "file", forbidden);
    }
}

struct Checker<'a> {
    sink: &'a mut Sink,
}

impl Checker<'_> {
    fn check_ident(&mut self, ident: &Ident, kind: &str, style: NameStyle) {
        let at = ident.span().start().into();
        self.check_name(at, &ident.unraw().to_string(), kind, style);
    }

    fn check_name(&mut self, at: Position, name: &str, kind: &str, style: NameStyle) {
        if name == "_" {
            return;
        }
        let found = FORBIDDEN.iter().find(|forbidden| match style {
            NameStyle::Camel => camel_has_suffix(name, forbidden.camel),
            NameStyle::Delimited => delimited_has_suffix(name, forbidden.snake),
        });
        if let Some(forbidden) = found {
            report(self.sink, at, name, kind, forbidden);
        }
    }

    /// 本体を持たない関数の署名。引数の識別子は呼び出し側が別のラベルで検査する。
    fn walk_signature_types(&mut self, signature: &Signature) {
        self.visit_generics(&signature.generics);
        for input in &signature.inputs {
            match input {
                FnArg::Receiver(receiver) => self.visit_receiver(receiver),
                FnArg::Typed(typed) => self.visit_type(&typed.ty),
            }
        }
        self.visit_return_type(&signature.output);
    }
}

impl<'ast> Visit<'ast> for Checker<'_> {
    fn visit_item(&mut self, node: &'ast Item) {
        if let Some((ident, kind, style)) = item_name(node) {
            self.check_ident(ident, kind, style);
        }
        visit::visit_item(self, node);
    }

    fn visit_trait_item_const(&mut self, node: &'ast TraitItemConst) {
        self.check_ident(&node.ident, "associated const", NameStyle::Delimited);
        visit::visit_trait_item_const(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        self.check_ident(&node.sig.ident, "trait method", NameStyle::Delimited);
        if node.default.is_some() {
            visit::visit_trait_item_fn(self, node);
            return;
        }
        // 本体の無い関数の引数は、パターンではなく識別子として扱われる（`parameter`）。
        for input in &node.sig.inputs {
            if let FnArg::Typed(typed) = input
                && let syn::Pat::Ident(pat) = &*typed.pat
            {
                self.check_ident(&pat.ident, "parameter", NameStyle::Delimited);
            }
        }
        self.walk_signature_types(&node.sig);
    }

    fn visit_trait_item_type(&mut self, node: &'ast TraitItemType) {
        self.check_ident(&node.ident, "associated type", NameStyle::Camel);
        visit::visit_trait_item_type(self, node);
    }

    fn visit_impl_item_const(&mut self, node: &'ast ImplItemConst) {
        self.check_ident(&node.ident, "associated const", NameStyle::Delimited);
        visit::visit_impl_item_const(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.check_ident(&node.sig.ident, "method", NameStyle::Delimited);
        visit::visit_impl_item_fn(self, node);
    }

    fn visit_impl_item_type(&mut self, node: &'ast ImplItemType) {
        self.check_ident(&node.ident, "associated type", NameStyle::Camel);
        visit::visit_impl_item_type(self, node);
    }

    fn visit_foreign_item_fn(&mut self, node: &'ast ForeignItemFn) {
        // 旧 lint には外部関数アイテムのフックが無く、引数もパターンではない。
        // 検査されるのは、型の中の関数ポインタと、ジェネリクスだけである。
        self.walk_signature_types(&node.sig);
    }

    fn visit_field(&mut self, node: &'ast Field) {
        if let Some(ident) = &node.ident {
            self.check_ident(ident, "field", NameStyle::Delimited);
        }
        visit::visit_field(self, node);
    }

    fn visit_variant(&mut self, node: &'ast Variant) {
        self.check_ident(&node.ident, "variant", NameStyle::Camel);
        visit::visit_variant(self, node);
    }

    fn visit_generic_param(&mut self, node: &'ast GenericParam) {
        match node {
            GenericParam::Lifetime(param) => {
                let at = param.lifetime.span().start().into();
                let name = format!("'{}", param.lifetime.ident.unraw());
                self.check_name(at, &name, "lifetime", NameStyle::Delimited);
            }
            GenericParam::Type(param) => {
                self.check_ident(&param.ident, "generic parameter", NameStyle::Camel);
            }
            GenericParam::Const(param) => {
                self.check_ident(&param.ident, "const parameter", NameStyle::Delimited);
            }
        }
        visit::visit_generic_param(self, node);
    }

    fn visit_bare_fn_arg(&mut self, node: &'ast BareFnArg) {
        if let Some((ident, _)) = &node.name {
            self.check_ident(ident, "function pointer parameter", NameStyle::Delimited);
        }
        visit::visit_bare_fn_arg(self, node);
    }

    fn visit_pat_ident(&mut self, node: &'ast PatIdent) {
        if is_binding(node) {
            self.check_ident(&node.ident, "variable", NameStyle::Delimited);
        }
        visit::visit_pat_ident(self, node);
    }
}

/// The name, label, and style of the items the old lint checked by name.
/// `use`, `extern crate`, `extern` blocks, and `impl` blocks have no name to check.
fn item_name(item: &Item) -> Option<(&Ident, &'static str, NameStyle)> {
    match item {
        Item::Static(item) => Some((&item.ident, "static", NameStyle::Delimited)),
        Item::Const(item) => Some((&item.ident, "const", NameStyle::Delimited)),
        Item::Fn(item) => Some((&item.sig.ident, "function", NameStyle::Delimited)),
        // 識別子を持つ `Item::Macro` は `macro_rules!` の定義。識別子の無いものは呼び出し。
        Item::Macro(item) => item
            .ident
            .as_ref()
            .map(|ident| (ident, "macro", NameStyle::Delimited)),
        Item::Mod(item) => Some((&item.ident, "module", NameStyle::Delimited)),
        Item::Type(item) => Some((&item.ident, "type alias", NameStyle::Camel)),
        Item::Enum(item) => Some((&item.ident, "enum", NameStyle::Camel)),
        Item::Struct(item) => Some((&item.ident, "struct", NameStyle::Camel)),
        Item::Union(item) => Some((&item.ident, "union", NameStyle::Camel)),
        Item::Trait(item) => Some((&item.ident, "trait", NameStyle::Camel)),
        Item::TraitAlias(item) => Some((&item.ident, "trait alias", NameStyle::Camel)),
        _ => None,
    }
}

/// syn は、定数・単位構造体・単位バリアントの名前だけのパターンを束縛と区別できない。
/// `mut`、`ref`、`@` が付いていれば束縛で、無ければ大文字で始まる名前をパスとみなす。
/// rustc は名前解決でこれを決めていた。
fn is_binding(pat: &PatIdent) -> bool {
    pat.by_ref.is_some()
        || pat.mutability.is_some()
        || pat.subpat.is_some()
        || !pat.ident.to_string().starts_with(char::is_uppercase)
}

fn report(sink: &mut Sink, at: Position, name: &str, kind: &str, forbidden: &Forbidden) {
    let suffix = forbidden.camel;
    let alternatives = forbidden.alternatives;
    sink.report(
        at,
        format!(
            "`{name}` ({kind}) は曖昧なサフィックス `{suffix}` を含んでいます。\
             `{suffix}` は責務の境界が不明確になりやすいサフィックスです。代替案: {alternatives}。\
             判定基準: この名前だけで「何に依存してよいか」「責務を一文で説明できるか」を確認してください。\
             外部API/フレームワーク由来の名前であれば `// syn-check: allow(ambiguous-suffix) -- <理由>` で明示的に許可してください"
        ),
    );
}

fn camel_has_suffix(name: &str, suffix: &str) -> bool {
    name == suffix || name.ends_with(suffix)
}

fn delimited_has_suffix(name: &str, suffix: &str) -> bool {
    let normalized = name.trim_start_matches('\'').to_ascii_lowercase();
    snake_has_suffix(&normalized, suffix)
}

fn snake_has_suffix(name: &str, suffix: &str) -> bool {
    name == suffix
        || name
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.ends_with('_'))
}

fn should_ignore(path: &Path) -> bool {
    if path.extension().is_none_or(|extension| extension != "rs") {
        return true;
    }

    if path
        .components()
        .any(|component| component.as_os_str() == "target")
    {
        return true;
    }

    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        if component.as_os_str() == "tests" {
            if components
                .peek()
                .is_some_and(|next| next.as_os_str() == "ui")
            {
                continue;
            }
            return true;
        }
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name == "tests.rs" || name.ends_with("_test.rs") || name.ends_with("_tests.rs")
        })
}
