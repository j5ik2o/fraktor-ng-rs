use std::collections::HashSet;

use quote::ToTokens;
use syn::{
  Ident, Visibility,
  visit::{self, Visit},
};

use crate::{
  ast, check_context::CheckContext, paths, position::Position, rule::Rule, sink::Sink, source_file::SourceFile,
};

/// Allows one public struct, enum, or trait per file. Ported from the old
/// `type-per-file-lint` (`multiple_type_definitions`).
pub(crate) struct TypePerFile;

impl Rule for TypePerFile {
  fn id(&self) -> &'static str {
    "type-per-file"
  }

  fn check(&self, file: &SourceFile, _cx: &CheckContext<'_>, sink: &mut Sink) {
    if paths::is_test_or_build_output(file.path()) {
      return;
    }
    let Some(ast) = file.ast() else {
      return;
    };
    let mut collector = Collector::default();
    collector.visit_file(ast);

    let mut counted: Vec<&TypeRecord> = Vec::new();
    let mut seen = HashSet::new();
    for record in &collector.records {
      // 例外規定：同じスコープに同名の型が複数あるなら、cfg で排他になっている。
      // 旧 lint は cfg 展開後の 1 つだけを見ていたので、ここでも 1 つとして数える。
      if !seen.insert((&record.scope, &record.name)) {
        continue;
      }
      if let Some(first) = counted.first() {
        let list: Vec<String> = counted.iter().chain([&record]).map(|record| record.describe()).collect();
        sink.report(
                    record.at,
                    format!(
                        "構造体・列挙型・トレイトはファイルごとに1つまでにしてください。このファイルでは既に {} が定義されています。検出された型一覧: {}",
                        first.describe(),
                        list.join(", ")
                    ),
                );
      }
      counted.push(record);
    }
  }
}

struct TypeRecord {
  /// The inline modules and blocks that enclose the type, outermost first.
  scope: Vec<String>,
  name: String,
  kind: &'static str,
  at: Position,
}

impl TypeRecord {
  fn describe(&self) -> String {
    format!("`{}` ({})", self.name, self.kind)
  }
}

#[derive(Default)]
struct Collector {
  scope: Vec<String>,
  blocks: usize,
  records: Vec<TypeRecord>,
}

impl Collector {
  fn record<T: ToTokens>(&mut self, vis: &Visibility, ident: &Ident, kind: &'static str, node: &T) {
    if matches!(vis, Visibility::Public(_)) {
      self.records.push(TypeRecord {
        scope: self.scope.clone(),
        name: ident.to_string(),
        kind,
        at: ast::decl_start(node),
      });
    }
  }
}

impl<'ast> Visit<'ast> for Collector {
  fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
    self.scope.push(node.ident.to_string());
    visit::visit_item_mod(self, node);
    self.scope.pop();
  }

  fn visit_block(&mut self, node: &'ast syn::Block) {
    // ブロックごとに別のスコープにする。別の関数にある同名の型を 1 つに数えないため。
    self.blocks += 1;
    self.scope.push(format!("{{{}}}", self.blocks));
    visit::visit_block(self, node);
    self.scope.pop();
  }

  fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
    self.record(&node.vis, &node.ident, "struct", node);
    visit::visit_item_struct(self, node);
  }

  fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
    self.record(&node.vis, &node.ident, "enum", node);
    visit::visit_item_enum(self, node);
  }

  fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
    self.record(&node.vis, &node.ident, "trait", node);
    visit::visit_item_trait(self, node);
  }
}
