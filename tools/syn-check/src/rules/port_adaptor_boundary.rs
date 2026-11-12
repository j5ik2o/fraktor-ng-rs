use std::{collections::HashMap, path::Path};

use syn::{
  Field, File, Item, ItemStruct, Path as SynPath, TypePath, UseTree, Visibility,
  visit::{self, Visit},
};

use crate::{ast, check_context::CheckContext, layout::Layout, paths, rule::Rule, sink::Sink, source_file::SourceFile};

/// Forbids a public struct in adaptor code from holding a concrete kernel
/// (core) API instead of a port trait. Ported from the old
/// `port-adaptor-boundary-lint` (`port_adaptor_boundary`), which already ran on syn.
pub(crate) struct PortAdaptorBoundary;

impl Rule for PortAdaptorBoundary {
  fn id(&self) -> &'static str {
    "port-adaptor-boundary"
  }

  fn check(&self, file: &SourceFile, cx: &CheckContext<'_>, sink: &mut Sink) {
    if !is_adaptor_source(file.path(), cx.layout) {
      return;
    }
    let Some(ast) = file.ast() else {
      return;
    };
    let imports = collect_core_imports(ast, cx.layout);
    for item in &ast.items {
      if let Item::Struct(item_struct) = item
        && matches!(item_struct.vis, Visibility::Public(_))
      {
        report_struct(item_struct, &imports, cx.layout, sink);
      }
    }
  }
}

fn report_struct(item_struct: &ItemStruct, imports: &HashMap<String, CoreImport>, layout: Layout, sink: &mut Sink) {
  let struct_name = item_struct.ident.to_string();
  for field in &item_struct.fields {
    if let Some(usage) = core_concrete_usage(field, imports, layout, &struct_name) {
      sink.report(ast::start(field), usage.message(layout));
    }
  }
}

/// The first concrete core type that `field` holds, anywhere in its type.
fn core_concrete_usage(
  field: &Field,
  imports: &HashMap<String, CoreImport>,
  layout: Layout,
  struct_name: &str,
) -> Option<CoreConcreteUsage> {
  struct TypeVisitor<'a> {
    imports: &'a HashMap<String, CoreImport>,
    layout: Layout,
    struct_name: &'a str,
    field_name: Option<String>,
    usage: Option<CoreConcreteUsage>,
  }

  impl<'ast> Visit<'ast> for TypeVisitor<'_> {
    fn visit_type_path(&mut self, node: &'ast TypePath) {
      if self.usage.is_some() {
        return;
      }
      self.usage =
        classify_type_path(&node.path, self.imports, self.layout, self.struct_name, self.field_name.as_deref());
      if self.usage.is_none() {
        visit::visit_type_path(self, node);
      }
    }
  }

  let mut visitor = TypeVisitor {
    imports,
    layout,
    struct_name,
    field_name: field.ident.as_ref().map(ToString::to_string),
    usage: None,
  };
  visitor.visit_type(&field.ty);
  visitor.usage
}

fn classify_type_path(
  path: &SynPath,
  imports: &HashMap<String, CoreImport>,
  layout: Layout,
  struct_name: &str,
  field_name: Option<&str>,
) -> Option<CoreConcreteUsage> {
  let segments: Vec<String> = path.segments.iter().map(|segment| segment.ident.to_string()).collect();
  let last = segments.last()?.clone();

  if is_core_path(&segments, layout) {
    if field_name == Some("inner") && last == struct_name {
      return Some(CoreConcreteUsage::SameNameWrapper { type_name: last });
    }
    if last.ends_with("Api") {
      return Some(CoreConcreteUsage::CoreApi { type_name: last });
    }
    return None;
  }

  if segments.len() != 1 {
    return None;
  }

  let import = imports.get(&last)?;
  let same_name_core_alias = field_name == Some("inner")
    && last == format!("Core{struct_name}")
    && (import.original == struct_name || import.original.trim_end_matches("Api") == struct_name);
  if same_name_core_alias {
    return Some(CoreConcreteUsage::SameNameWrapper { type_name: import.original.clone() });
  }
  if import.original.ends_with("Api") {
    return Some(CoreConcreteUsage::CoreApi { type_name: import.original.clone() });
  }
  None
}

/// The names that the top-level `use` items bring in from core, keyed by the
/// local name (the alias when there is one).
fn collect_core_imports(file: &File, layout: Layout) -> HashMap<String, CoreImport> {
  let mut imports = HashMap::new();
  for item in &file.items {
    if let Item::Use(item_use) = item {
      collect_use_tree(&item_use.tree, &mut Vec::new(), layout, &mut imports);
    }
  }
  imports
}

fn collect_use_tree(
  tree: &UseTree,
  prefix: &mut Vec<String>,
  layout: Layout,
  imports: &mut HashMap<String, CoreImport>,
) {
  let (original, local) = match tree {
    | UseTree::Path(path) => {
      prefix.push(path.ident.to_string());
      collect_use_tree(&path.tree, prefix, layout, imports);
      prefix.pop();
      return;
    },
    | UseTree::Group(group) => {
      for item in &group.items {
        collect_use_tree(item, prefix, layout, imports);
      }
      return;
    },
    | UseTree::Glob(_) => return,
    | UseTree::Name(name) => (&name.ident, &name.ident),
    | UseTree::Rename(rename) => (&rename.ident, &rename.rename),
  };
  prefix.push(original.to_string());
  if is_core_path(prefix, layout) {
    imports.insert(local.to_string(), CoreImport { original: original.to_string() });
  }
  prefix.pop();
}

/// Whether a path names something in core: another crate's core in the old
/// layout, the kernel module of this crate or another crate in ng.
fn is_core_path(segments: &[String], layout: Layout) -> bool {
  match layout {
    | Layout::Legacy => segments.first().is_some_and(|root| root.starts_with("fraktor_") && root.contains("_core")),
    | Layout::Ng => match segments {
      | [root, module, ..] => (root == "crate" || root.starts_with("fraktor_")) && module == "kernel",
      | _ => false,
    },
  }
}

fn is_adaptor_source(path: &Path, layout: Layout) -> bool {
  match layout {
    | Layout::Legacy => {
      // 旧 lint は絶対パスで判定していた。相対パスの先頭が `modules/` でも同じ結果になるよう `/` を前に置く。
      let normalized = format!("/{}", path.to_string_lossy().replace('\\', "/"));
      normalized.contains("/modules/") && normalized.contains("-adaptor-std/src/")
    },
    | Layout::Ng => paths::is_ng_module_source(path, "adaptor"),
  }
}

struct CoreImport {
  original: String,
}

enum CoreConcreteUsage {
  CoreApi { type_name: String },
  SameNameWrapper { type_name: String },
}

impl CoreConcreteUsage {
  fn message(&self, layout: Layout) -> String {
    let (adaptor, core) = match layout {
      | Layout::Legacy => ("std adapter", "core"),
      | Layout::Ng => ("adaptor", "kernel"),
    };
    let primary = match self {
      | Self::CoreApi { type_name } => {
        format!("{adaptor} の public struct が {core} concrete API `{type_name}` を保持しています")
      },
      | Self::SameNameWrapper { type_name } => {
        format!("{adaptor} の public struct が {core} concrete `{type_name}` を同名 wrapper として保持しています")
      },
    };
    format!(
      "{primary}。{core} 側に必要最小の port trait を定義し、{core} の concrete API にそれを実装させ、{adaptor} は concrete API ではなく port trait に依存してください"
    )
  }
}
