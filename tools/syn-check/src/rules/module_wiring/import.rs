//! Splits a `use` item into the imports rustc lowers it to.
//!
//! rustc turns `use a::{b, c::*};` into one HIR item per leaf of the tree
//! (`a::b` and the glob `a::c`), plus a list stem that the old lint skipped.
//! A leaf inside braces is reported at its own position.

use syn::{Ident, ItemUse, UseTree, ext::IdentExt};

use crate::{ast, position::Position};

/// The segment that stands for a leading `::`.
const ROOT_SEGMENT: &str = "::";

pub(super) struct Import {
  pub(super) segments: Vec<String>,
  pub(super) kind: ImportKind,
  pub(super) at: Position,
}

pub(super) enum ImportKind {
  /// Binds one name: the last segment, or the name after `as`.
  Single { binding: String },
  /// `path::*`. The segments are the path before `*`.
  Glob,
}

pub(super) fn lower(item: &ItemUse) -> Vec<Import> {
  let mut prefix = Vec::new();
  if item.leading_colon.is_some() {
    prefix.push(ROOT_SEGMENT.to_string());
  }
  let mut imports = Vec::new();
  lower_tree(&item.tree, prefix, ast::decl_start(item), &mut imports);
  imports
}

fn lower_tree(tree: &UseTree, mut prefix: Vec<String>, at: Position, imports: &mut Vec<Import>) {
  match tree {
    | UseTree::Path(path) => {
      prefix.push(name(&path.ident));
      lower_tree(&path.tree, prefix, at, imports);
    },
    | UseTree::Name(leaf) => imports.push(single(prefix, &leaf.ident, None, at)),
    | UseTree::Rename(leaf) => {
      imports.push(single(prefix, &leaf.ident, Some(&leaf.rename), at));
    },
    | UseTree::Glob(_) => imports.push(Import { segments: prefix, kind: ImportKind::Glob, at }),
    | UseTree::Group(group) => {
      for nested in &group.items {
        lower_tree(nested, prefix.clone(), ast::start(nested), imports);
      }
    },
  }
}

fn single(mut segments: Vec<String>, ident: &Ident, rename: Option<&Ident>, at: Position) -> Import {
  segments.push(name(ident));
  // rustc は `a::{self}` を `a` の import にする。束縛名も `a` になる。
  if segments.len() > 1 && segments.last().is_some_and(|last| last == "self") {
    segments.pop();
  }
  let binding = match rename {
    | Some(rename) => name(rename),
    | None => segments.last().cloned().unwrap_or_default(),
  };
  Import { segments, kind: ImportKind::Single { binding }, at }
}

fn name(ident: &Ident) -> String {
  ident.unraw().to_string()
}
