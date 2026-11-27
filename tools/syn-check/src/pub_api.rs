// SPDX-License-Identifier: Apache-2.0

//! Public API item extractor built on `syn`.
//!
//! Walks a crate from `src/lib.rs`, follows both `pub mod` and private leaf
//! modules whose items are re-exported via `pub use`, and emits canonical
//! public API lines suitable for PR diffing.

mod format;
mod resolve;

use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  path::{Path, PathBuf},
};

use syn::{Attribute, ImplItem, Item, ItemImpl, ItemMod, UseTree, Visibility};

use crate::paths::is_test_file_name;

#[derive(Debug)]
pub(crate) struct ModuleGraph {
  pub(crate) crate_name: String,
  pub(crate) modules: Vec<ModuleData>,
  pub(crate) by_path: BTreeMap<Vec<String>, usize>,
  pub(crate) macro_exports: BTreeSet<String>,
  /// How many structs, enums, unions, and type aliases of each name the
  /// crate defines, at any visibility. An inherent `impl` outside the
  /// defining module is attributed to a type only when its name is unique.
  pub(crate) type_name_counts: BTreeMap<String, usize>,
}

#[derive(Debug)]
pub(crate) struct ModuleData {
  pub(crate) path: Vec<String>,
  pub(crate) submodules: Vec<SubmoduleEdge>,
  pub(crate) pub_uses: Vec<PubUse>,
  pub(crate) pub_items: Vec<PubItem>,
  pub(crate) inherent_impls: BTreeMap<String, Vec<MemberSignature>>,
}

#[derive(Debug)]
pub(crate) struct SubmoduleEdge {
  pub(crate) name: String,
  pub(crate) is_pub: bool,
  pub(crate) target: usize,
}

#[derive(Debug)]
pub(crate) struct PubUse {
  pub(crate) segments: Vec<String>,
  pub(crate) kind: UseKind,
}

#[derive(Debug)]
pub(crate) enum UseKind {
  Single { alias: String },
  Glob,
}

#[derive(Debug, Clone)]
pub(crate) struct PubItem {
  pub(crate) name: String,
  pub(crate) kind_prefix: String,
  pub(crate) suffix: String,
  pub(crate) members: Vec<MemberSignature>,
}

#[derive(Debug, Clone)]
pub(crate) struct MemberSignature {
  pub(crate) kind_prefix: String,
  pub(crate) member_name: String,
  pub(crate) suffix: String,
}

/// Collects the sorted canonical public API lines for a crate directory.
pub fn collect_crate(crate_dir: &Path) -> Result<Vec<String>, String> {
  let lib_rs = crate_dir.join("src").join("lib.rs");
  if !lib_rs.is_file() {
    return Ok(Vec::new());
  }
  let crate_name = read_crate_name(crate_dir)?;
  let mut graph = ModuleGraph {
    crate_name: crate_name.clone(),
    modules: Vec::new(),
    by_path: BTreeMap::new(),
    macro_exports: BTreeSet::new(),
    type_name_counts: BTreeMap::new(),
  };
  let root_path = vec![crate_name];
  let root_id = load_file_module(&mut graph, &lib_rs, root_path, true)?;
  Ok(resolve::emit_public_items(&graph, root_id))
}

fn read_crate_name(crate_dir: &Path) -> Result<String, String> {
  let manifest = crate_dir.join("Cargo.toml");
  if let Ok(content) = fs::read_to_string(&manifest) {
    let mut in_package = false;
    for raw_line in content.lines() {
      let line = raw_line.trim();
      if line.starts_with('[') {
        in_package = line == "[package]";
        continue;
      }
      if in_package && let Some(rest) = line.strip_prefix("name") {
        let rest = rest.trim_start();
        if let Some(val) = rest.strip_prefix('=') {
          let trimmed = val.trim().trim_matches('"').trim_matches('\'');
          if !trimmed.is_empty() {
            return Ok(trimmed.replace('-', "_"));
          }
        }
      }
    }
  }
  let fallback = crate_dir
    .file_name()
    .and_then(|name| name.to_str())
    .ok_or_else(|| format!("cannot determine crate name for {}", crate_dir.display()))?;
  Ok(fallback.replace('-', "_"))
}

fn load_file_module(
  graph: &mut ModuleGraph,
  file_path: &Path,
  mod_path: Vec<String>,
  is_mod_root: bool,
) -> Result<usize, String> {
  let source = fs::read_to_string(file_path).map_err(|err| format!("failed to read {}: {err}", file_path.display()))?;
  let parsed = syn::parse_file(&source).map_err(|err| format!("failed to parse {}: {err}", file_path.display()))?;
  let mod_id = allocate_module(graph, mod_path.clone());
  let base_dir = module_child_dir(file_path, is_mod_root);
  collect_items(graph, mod_id, &parsed.items, &base_dir, &mod_path)?;
  Ok(mod_id)
}

fn allocate_module(graph: &mut ModuleGraph, mod_path: Vec<String>) -> usize {
  let id = graph.modules.len();
  graph.by_path.insert(mod_path.clone(), id);
  graph.modules.push(ModuleData {
    path: mod_path,
    submodules: Vec::new(),
    pub_uses: Vec::new(),
    pub_items: Vec::new(),
    inherent_impls: BTreeMap::new(),
  });
  id
}

fn module_child_dir(file_path: &Path, is_mod_root: bool) -> PathBuf {
  let parent = file_path.parent().unwrap_or_else(|| Path::new("."));
  if is_mod_root {
    parent.to_path_buf()
  } else {
    let stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    parent.join(stem)
  }
}

fn collect_items(
  graph: &mut ModuleGraph,
  mod_id: usize,
  items: &[Item],
  base_dir: &Path,
  mod_path: &[String],
) -> Result<(), String> {
  for item in items {
    if has_cfg_test(item_attrs(item)) {
      continue;
    }
    match item {
      | Item::Mod(item_mod) => handle_mod(graph, mod_id, item_mod, base_dir, mod_path)?,
      | Item::Use(item_use) if is_public(&item_use.vis) => {
        let mut uses = Vec::new();
        let mut prefix = Vec::new();
        flatten_use_tree(&item_use.tree, &mut prefix, &mut uses);
        graph.modules[mod_id].pub_uses.extend(uses);
      },
      | Item::Impl(item_impl) => handle_impl(graph, mod_id, item_impl),
      | Item::Macro(item_macro) => {
        if has_macro_export(&item_macro.attrs)
          && let Some(ident) = &item_macro.ident
        {
          graph.macro_exports.insert(ident.to_string());
        }
      },
      | other => {
        if let Some(name) = type_name(other) {
          *graph.type_name_counts.entry(name).or_default() += 1;
        }
        if let Some(pub_item) = format::extract_pub_item(other) {
          graph.modules[mod_id].pub_items.push(pub_item);
        }
      },
    }
  }
  Ok(())
}

fn handle_mod(
  graph: &mut ModuleGraph,
  mod_id: usize,
  item_mod: &ItemMod,
  base_dir: &Path,
  mod_path: &[String],
) -> Result<(), String> {
  let name = item_mod.ident.to_string();
  let is_pub = is_public(&item_mod.vis);
  let mut child_path = mod_path.to_vec();
  child_path.push(name.clone());

  if let Some((_, inline_items)) = &item_mod.content {
    let child_id = allocate_module(graph, child_path.clone());
    let inline_dir = base_dir.join(&name);
    collect_items(graph, child_id, inline_items, &inline_dir, &child_path)?;
    graph.modules[mod_id].submodules.push(SubmoduleEdge { name, is_pub, target: child_id });
    return Ok(());
  }

  let Some((child_file, child_is_root)) = resolve_mod_file(base_dir, &name, &item_mod.attrs) else {
    return Ok(());
  };
  if is_test_file_name(&child_file) {
    return Ok(());
  }
  let child_id = load_file_module(graph, &child_file, child_path, child_is_root)?;
  graph.modules[mod_id].submodules.push(SubmoduleEdge { name, is_pub, target: child_id });
  Ok(())
}

fn resolve_mod_file(base_dir: &Path, name: &str, attrs: &[Attribute]) -> Option<(PathBuf, bool)> {
  if let Some(explicit) = extract_path_attr(attrs) {
    let candidate = base_dir.join(&explicit);
    if candidate.is_file() {
      return Some((candidate, false));
    }
    if let Some(parent) = base_dir.parent() {
      let sibling = parent.join(&explicit);
      if sibling.is_file() {
        return Some((sibling, false));
      }
    }
    return None;
  }
  let file_candidate = base_dir.join(format!("{name}.rs"));
  if file_candidate.is_file() {
    return Some((file_candidate, false));
  }
  let mod_candidate = base_dir.join(name).join("mod.rs");
  if mod_candidate.is_file() {
    return Some((mod_candidate, true));
  }
  None
}

fn extract_path_attr(attrs: &[Attribute]) -> Option<String> {
  for attr in attrs {
    if !attr.path().is_ident("path") {
      continue;
    }
    if let syn::Meta::NameValue(nv) = &attr.meta
      && let syn::Expr::Lit(expr_lit) = &nv.value
      && let syn::Lit::Str(lit_str) = &expr_lit.lit
    {
      return Some(lit_str.value());
    }
  }
  None
}

fn handle_impl(graph: &mut ModuleGraph, mod_id: usize, item_impl: &ItemImpl) {
  if item_impl.trait_.is_some() {
    return;
  }
  let Some(type_name) = format::impl_target_ident(&item_impl.self_ty) else {
    return;
  };
  let mut members = Vec::new();
  for impl_item in &item_impl.items {
    if has_cfg_test(impl_item_attrs(impl_item)) {
      continue;
    }
    if let Some(member) = format::extract_pub_impl_item(impl_item) {
      members.push(member);
    }
  }
  if !members.is_empty() {
    graph.modules[mod_id].inherent_impls.entry(type_name).or_default().extend(members);
  }
}

fn flatten_use_tree(tree: &UseTree, prefix: &mut Vec<String>, out: &mut Vec<PubUse>) {
  match tree {
    | UseTree::Path(path) => {
      prefix.push(path.ident.to_string());
      flatten_use_tree(&path.tree, prefix, out);
      prefix.pop();
    },
    | UseTree::Name(name) => {
      let ident = name.ident.to_string();
      let mut segments = prefix.clone();
      if ident == "self" {
        if let Some(last) = segments.last().cloned() {
          out.push(PubUse { segments, kind: UseKind::Single { alias: last } });
        }
      } else {
        segments.push(ident.clone());
        out.push(PubUse { segments, kind: UseKind::Single { alias: ident } });
      }
    },
    | UseTree::Rename(rename) => {
      let ident = rename.ident.to_string();
      let alias = rename.rename.to_string();
      if alias == "_" {
        return;
      }
      let mut segments = prefix.clone();
      if ident != "self" {
        segments.push(ident);
      }
      out.push(PubUse { segments, kind: UseKind::Single { alias } });
    },
    | UseTree::Glob(_) => {
      out.push(PubUse { segments: prefix.clone(), kind: UseKind::Glob });
    },
    | UseTree::Group(group) => {
      for item in &group.items {
        flatten_use_tree(item, prefix, out);
      }
    },
  }
}

pub(crate) fn is_public(vis: &Visibility) -> bool {
  matches!(vis, Visibility::Public(_))
}

pub(crate) fn has_cfg_test(attrs: &[Attribute]) -> bool {
  attrs.iter().any(|attr| {
    if !attr.path().is_ident("cfg") {
      return false;
    }
    let mut is_test = false;
    let _parsed = attr.parse_nested_meta(|meta| {
      if meta.path.is_ident("test") {
        is_test = true;
      } else if meta.path.is_ident("all") {
        let _inner = meta.parse_nested_meta(|inner| {
          if inner.path.is_ident("test") {
            is_test = true;
          }
          Ok(())
        });
      }
      Ok(())
    });
    is_test
  })
}

fn type_name(item: &Item) -> Option<String> {
  match item {
    | Item::Struct(x) => Some(x.ident.to_string()),
    | Item::Enum(x) => Some(x.ident.to_string()),
    | Item::Union(x) => Some(x.ident.to_string()),
    | Item::Type(x) => Some(x.ident.to_string()),
    | _ => None,
  }
}

fn has_macro_export(attrs: &[Attribute]) -> bool {
  attrs.iter().any(|attr| attr.path().is_ident("macro_export"))
}

fn item_attrs(item: &Item) -> &[Attribute] {
  match item {
    | Item::Const(x) => &x.attrs,
    | Item::Enum(x) => &x.attrs,
    | Item::ExternCrate(x) => &x.attrs,
    | Item::Fn(x) => &x.attrs,
    | Item::ForeignMod(x) => &x.attrs,
    | Item::Impl(x) => &x.attrs,
    | Item::Macro(x) => &x.attrs,
    | Item::Mod(x) => &x.attrs,
    | Item::Static(x) => &x.attrs,
    | Item::Struct(x) => &x.attrs,
    | Item::Trait(x) => &x.attrs,
    | Item::TraitAlias(x) => &x.attrs,
    | Item::Type(x) => &x.attrs,
    | Item::Union(x) => &x.attrs,
    | Item::Use(x) => &x.attrs,
    | _ => &[],
  }
}

fn impl_item_attrs(item: &ImplItem) -> &[Attribute] {
  match item {
    | ImplItem::Const(x) => &x.attrs,
    | ImplItem::Fn(x) => &x.attrs,
    | ImplItem::Type(x) => &x.attrs,
    | ImplItem::Macro(x) => &x.attrs,
    | _ => &[],
  }
}
