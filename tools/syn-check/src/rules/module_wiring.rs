mod import;
mod module_dir;
mod module_items;
mod source_text;

use std::collections::BTreeSet;

use syn::{
  Attribute, Item, ItemMod, ItemUse, Visibility,
  ext::IdentExt,
  spanned::Spanned,
  visit::{self, Visit},
};

use crate::{
  ast, check_context::CheckContext, position::Position, rule::Rule, sink::Sink, source_file::SourceFile,
  workspace::Workspace,
};
use import::{Import, ImportKind};
use module_dir::{FileModule, ModuleDir, PathAttribute};
use module_items::Leafness;

/// Restricts how modules are wired: only the direct parent of a leaf module
/// may re-export from it, a leaf module is declared with a bare `mod`, and
/// modules are wired by file layout alone (no `#[path]`, no `include!`).
/// Ported from the old `module-wiring-lint` (`no_parent_reexport`).
pub(crate) struct ModuleWiring;

impl Rule for ModuleWiring {
  fn id(&self) -> &'static str {
    "module-wiring"
  }

  fn check(&self, file: &SourceFile, cx: &CheckContext<'_>, sink: &mut Sink) {
    let Some(ast) = file.ast() else {
      return;
    };
    let root = FileModule::of(file, cx.workspace);
    let mut walker = Walker {
      file,
      workspace: cx.workspace,
      sink,
      scopes: vec![Scope { name: root.name, dir: root.dir, items: &ast.items }],
      reported_includes: BTreeSet::new(),
    };
    walker.visit_file(ast);
  }
}

const REEXPORT: &str = "再エクスポートは末端モジュールの直属親以外では禁止です";
const PRINCIPLE: &str = "原則ルール: 再エクスポート禁止";
const EXCEPTION: &str = "例外ルール: 末端モジュールの直属親のみ許可";
const WIRING: &str = "原則ルール: モジュールはファイル配置と `mod` 宣言だけで結線してください";

/// The module that encloses the items being visited.
struct Scope<'ast> {
  /// `None` for a crate root.
  name: Option<String>,
  dir: ModuleDir,
  items: &'ast [Item],
}

struct Walker<'a, 'ast> {
  file: &'a SourceFile,
  workspace: &'a Workspace,
  sink: &'a mut Sink,
  scopes: Vec<Scope<'ast>>,
  reported_includes: BTreeSet<Position>,
}

impl<'ast> Visit<'ast> for Walker<'_, 'ast> {
  fn visit_item_mod(&mut self, node: &'ast ItemMod) {
    self.evaluate_mod(node);
    if let Some((_, items)) = &node.content {
      let dir = self.scope().dir.inline_child(node);
      self.scopes.push(Scope { name: Some(node.ident.unraw().to_string()), dir, items });
      visit::visit_item_mod(self, node);
      self.scopes.pop();
    }
  }

  fn visit_item_use(&mut self, node: &'ast ItemUse) {
    self.evaluate_use(node);
  }
}

impl Walker<'_, '_> {
  fn scope(&self) -> &Scope<'_> {
    self.scopes.last().expect("the file scope is never popped")
  }

  fn evaluate_mod(&mut self, module: &ItemMod) {
    let name = module.ident.unraw().to_string();
    let mut allowed_test_path = false;
    for attr in &module.attrs {
      let at = match PathAttribute::of(attr) {
        | None => continue,
        | Some(PathAttribute::Plain(path)) if self.is_sibling_test_hook(&name, attr, &path) => {
          allowed_test_path = true;
          continue;
        },
        | Some(PathAttribute::Plain(_)) => ast::start(attr),
        // 旧 lint は `#[path` の文字列を探して位置を決めていた。cfg_attr の中の path では
        // 見つからず、アイテムの位置で報告していた。
        | Some(PathAttribute::CfgAttr) => ast::decl_start(module),
      };
      self.sink.report(
                at,
                format!(
                    "モジュール宣言で `#[path = \"...\"]` 属性は禁止されています（{WIRING}）。詳細: 属性 `{}` が設定されています。モジュールファイルを規約どおりの場所に配置し、`mod foo;` として宣言してください",
                    self.snippet(ast::start(attr), attr.span().end().into())
                ),
            );
      return;
    }
    if allowed_test_path {
      return;
    }

    if let Some(at) = self.include_in(module) {
      // 旧 lint は外側のインラインモジュールでも同じ include! を見つけた。rustc は同じ診断を
      // 1 度しか出さないので、同じ位置は 1 件にまとめる。
      if self.reported_includes.insert(at) {
        self.sink.report(
                    at,
                    format!(
                        "モジュール内で `include!` マクロを用いた結線は禁止されています（{WIRING}）。詳細: マクロ `include!` によって外部ファイルを結線しようとしています。対象ファイルを所定のパスに配置し、`mod child;` でモジュールを宣言してください"
                    ),
                );
      }
      return;
    }

    if name == "prelude" {
      return;
    }
    if self.leafness(module) != Leafness::Leaf {
      return;
    }
    if matches!(module.vis, Visibility::Inherited) {
      return;
    }
    let at = ast::decl_start(module);
    self.sink.report(
            at,
            format!(
                "末端モジュールの宣言では `mod` のみを使用してください。詳細: 宣言 `{}` に公開可視性が設定されています。宣言から可視性修飾子を削除し、公開が必要なシンボルは末端モジュール内で `pub` を付けてください",
                self.snippet(at, module.ident.span().end().into())
            ),
        );
  }

  fn evaluate_use(&mut self, item: &ItemUse) {
    // 旧 is_public_use。親モジュールに限定された可視性は、書かれていれば再エクスポートとみなした。
    // 結局、可視性が明示されているかどうかで決まる。
    if matches!(item.vis, Visibility::Inherited) {
      return;
    }
    if self.scope().name.as_deref() == Some("prelude") {
      return;
    }
    for import in import::lower(item) {
      if let Some(message) = self.import_violation(&import) {
        self.sink.report(import.at, message);
      }
    }
  }

  fn import_violation(&self, import: &Import) -> Option<String> {
    let reexport =
      |label: &str, detail: &str, help: &str| Some(format!("{REEXPORT}（{label}）。詳細: {detail}。{help}"));
    if matches!(import.kind, ImportKind::Single { .. }) && import.segments.len() < 2 {
      return reexport(
        PRINCIPLE,
        "子モジュールを介さない再エクスポートは許可されていません",
        "末端モジュールにシンボルを配置し、親から `pub use child::Type;` として公開してください",
      );
    }
    let first = import.segments.first()?;
    if matches!(first.as_str(), "self" | "super" | "crate") {
      return reexport(
        PRINCIPLE,
        "特殊パス（self / super / crate）を経由した再エクスポートは禁止されています",
        "末端モジュールの直属親からのみ再エクスポートしてください",
      );
    }
    if let ImportKind::Single { binding } = &import.kind
      && import.segments.last() != Some(binding)
    {
      return reexport(
        EXCEPTION,
        "`as` を用いた再エクスポートは許可されていません",
        "末端モジュール内で元の名前を公開するか、呼び出し側で名前を付け替えてください",
      );
    }
    let Some(child) = module_items::declared_modules(self.scope().items)
      .into_iter()
      .find(|module| module.ident.unraw() == first.as_str())
    else {
      return reexport(
        PRINCIPLE,
        &format!("モジュール `{first}` はこのモジュールの直属の子として定義されていません"),
        "`mod child;` を定義した親モジュール内でのみ `pub use child::Type;` を記述してください",
      );
    };
    if self.leafness(child) == Leafness::Branch {
      return reexport(
        EXCEPTION,
        &format!("モジュール `{first}` にさらに子モジュールが存在するため末端モジュールではありません"),
        "再エクスポートは葉モジュールの直属親でのみ行い、階層が深い場合は子モジュール側で公開してください",
      );
    }
    None
  }

  /// Whether `module`, declared in the current scope, has child modules.
  fn leafness(&self, module: &ItemMod) -> Leafness {
    match &module.content {
      | Some((_, items)) => module_items::leafness(items),
      | None => self
        .scope()
        .dir
        .file_child(module, self.workspace)
        .and_then(|file| file.ast().map(|ast| module_items::leafness(&ast.items)))
        .unwrap_or(Leafness::Unknown),
    }
  }

  /// The old `is_allowed_sibling_test_path_attr`: `#[cfg(test)]` on the line
  /// right before `#[path = "<file stem>_test.rs"] mod tests;`.
  fn is_sibling_test_hook(&self, name: &str, attr: &Attribute, path: &str) -> bool {
    if name != "tests" {
      return false;
    }
    let text = self.file.text();
    let before = &text[..source_text::byte_offset(text, ast::start(attr))];
    if !source_text::ends_with_cfg_test_line(before) {
      return false;
    }
    if path.contains('/') || path.contains('\\') {
      return false;
    }
    self.file.path().file_stem().and_then(|stem| stem.to_str()).is_some_and(|stem| path == format!("{stem}_test.rs"))
  }

  /// The first `include!(` in the source text of `module`, attributes excluded.
  fn include_in(&self, module: &ItemMod) -> Option<Position> {
    let text = self.file.text();
    let start = source_text::byte_offset(text, ast::decl_start(module));
    let end = source_text::byte_offset(text, module.span().end().into());
    let found = source_text::find_include(text.get(start..end)?)?;
    Some(source_text::position(text, start + found))
  }

  fn snippet(&self, from: Position, to: Position) -> &str {
    let text = self.file.text();
    let start = source_text::byte_offset(text, from);
    let end = source_text::byte_offset(text, to);
    text.get(start..end).unwrap_or_default().trim()
  }
}
