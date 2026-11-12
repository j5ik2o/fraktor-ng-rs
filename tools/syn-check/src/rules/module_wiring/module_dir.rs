//! Where module files are, following rustc's module file lookup
//! (`rustc_expand::module`).

use std::{
  path::{Path, PathBuf},
  rc::Rc,
};

use syn::{Attribute, Expr, ExprLit, Item, ItemMod, Lit, Meta, Token, ext::IdentExt, punctuated::Punctuated};

use crate::{source_file::SourceFile, workspace::Workspace};

/// The directory where a module looks up the files of its `mod foo;`
/// declarations. rustc keeps the same pair as `dir_path` and
/// `DirOwnership::Owned { relative }`.
#[derive(Clone)]
pub(super) struct ModuleDir {
  dir: PathBuf,
  /// The module name of a non-mod-rs file `foo.rs`, whose children are
  /// under `foo/`. `None` for a crate root, a `mod.rs`, a file loaded with
  /// `#[path]`, and an inline module.
  relative: Option<String>,
}

impl ModuleDir {
  /// The directory of an inline module `mod foo { ... }` declared here.
  pub(super) fn inline_child(&self, module: &ItemMod) -> Self {
    let dir = match path_value(module) {
      // rustc は inline モジュールの #[path] をディレクトリとして扱い、relative を足さずに連結する。
      | Some(path) => self.dir.join(path),
      | None => self.children_dir().join(module.ident.unraw().to_string()),
    };
    Self { dir, relative: None }
  }

  /// The file of `mod foo;` declared here. `None` when no file is found or
  /// when both `foo.rs` and `foo/mod.rs` exist.
  pub(super) fn file_child(&self, module: &ItemMod, workspace: &Workspace) -> Option<Rc<SourceFile>> {
    if let Some(path) = path_value(module) {
      return workspace.source(&self.dir.join(path));
    }
    let name = module.ident.unraw().to_string();
    let base = self.children_dir();
    let default = workspace.source(&base.join(format!("{name}.rs")));
    let secondary = workspace.source(&base.join(&name).join("mod.rs"));
    match (default, secondary) {
      | (Some(file), None) | (None, Some(file)) => Some(file),
      | _ => None,
    }
  }

  fn children_dir(&self) -> PathBuf {
    match &self.relative {
      | Some(relative) => self.dir.join(relative),
      | None => self.dir.clone(),
    }
  }
}

/// What the checked file is in its module tree.
pub(super) struct FileModule {
  /// The module name, or `None` for a crate root.
  pub(super) name: Option<String>,
  pub(super) dir: ModuleDir,
}

impl FileModule {
  /// `lib.rs` and `main.rs` are crate roots. `mod.rs` is the module named
  /// after its directory. Any other `foo.rs` is the module `foo` when a
  /// parent file declares `mod foo;` for it, and a crate root otherwise.
  pub(super) fn of(file: &SourceFile, workspace: &Workspace) -> Self {
    let dir = file.path().parent().unwrap_or(Path::new("")).to_path_buf();
    let stem = file.path().file_stem().and_then(|stem| stem.to_str());
    match (file.file_name(), stem) {
      | ("mod.rs", _) => Self {
        name: dir.file_name().map(|name| name.to_string_lossy().into_owned()),
        dir: ModuleDir { dir, relative: None },
      },
      | ("lib.rs" | "main.rs", _) | (_, None) => Self::crate_root(dir),
      | (_, Some(stem)) if is_declared(&dir, stem, workspace) => {
        Self { name: Some(stem.to_string()), dir: ModuleDir { dir, relative: Some(stem.to_string()) } }
      },
      | _ => Self::crate_root(dir),
    }
  }

  fn crate_root(dir: PathBuf) -> Self {
    Self { name: None, dir: ModuleDir { dir, relative: None } }
  }
}

/// A `#[path]` on a module declaration.
pub(super) enum PathAttribute {
  /// `#[path = "..."]` with its value.
  Plain(String),
  /// `path = "..."` inside `#[cfg_attr(...)]`.
  CfgAttr,
}

impl PathAttribute {
  pub(super) fn of(attr: &Attribute) -> Option<Self> {
    if attr.path().is_ident("path") {
      return string_value(&attr.meta).map(Self::Plain);
    }
    (attr.path().is_ident("cfg_attr") && cfg_attr_sets_path(&attr.meta)).then_some(Self::CfgAttr)
  }
}

/// Whether some file in an ancestor directory declares `mod <stem>;` for the
/// file `<dir>/<stem>.rs`: a `lib.rs`, `main.rs` or `mod.rs`, or the
/// non-mod-rs file named after the directory, possibly through inline modules.
fn is_declared(dir: &Path, stem: &str, workspace: &Workspace) -> bool {
  let mut inline_path: Vec<String> = Vec::new();
  let mut ancestor = dir;
  loop {
    for mod_rs in ["lib.rs", "main.rs", "mod.rs"] {
      if declares(&ancestor.join(mod_rs), &inline_path, stem, workspace) {
        return true;
      }
    }
    let (Some(name), Some(parent)) = (ancestor.file_name().and_then(|name| name.to_str()), ancestor.parent()) else {
      return false;
    };
    if declares(&parent.join(format!("{name}.rs")), &inline_path, stem, workspace) {
      return true;
    }
    inline_path.insert(0, name.to_string());
    ancestor = parent;
  }
}

fn declares(file: &Path, inline_path: &[String], stem: &str, workspace: &Workspace) -> bool {
  workspace
    .source(file)
    .is_some_and(|source| source.ast().is_some_and(|ast| declares_in(&ast.items, inline_path, stem)))
}

fn declares_in(items: &[Item], inline_path: &[String], stem: &str) -> bool {
  let mut unpathed_mods = items.iter().filter_map(|item| match item {
    | Item::Mod(module) if path_value(module).is_none() => Some(module),
    | _ => None,
  });
  match inline_path.split_first() {
    | None => unpathed_mods.any(|module| module.content.is_none() && module.ident.unraw() == stem),
    | Some((next, rest)) => unpathed_mods.any(|module| {
      module.ident.unraw() == next.as_str()
        && module.content.as_ref().is_some_and(|(_, items)| declares_in(items, rest, stem))
    }),
  }
}

/// The value of a plain `#[path = "..."]` on `module`.
fn path_value(module: &ItemMod) -> Option<String> {
  module.attrs.iter().find(|attr| attr.path().is_ident("path")).and_then(|attr| string_value(&attr.meta))
}

fn string_value(meta: &Meta) -> Option<String> {
  match meta {
    | Meta::NameValue(name_value) => match &name_value.value {
      | Expr::Lit(ExprLit { lit: Lit::Str(value), .. }) => Some(value.value()),
      | _ => None,
    },
    | _ => None,
  }
}

/// Whether a `cfg_attr(<predicate>, <attr>...)` sets `path`, also through a
/// nested `cfg_attr`.
fn cfg_attr_sets_path(meta: &Meta) -> bool {
  let Meta::List(list) = meta else {
    return false;
  };
  list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated).is_ok_and(|metas| {
    metas.iter().skip(1).any(|meta| match meta {
      | Meta::NameValue(name_value) => name_value.path.is_ident("path"),
      | Meta::List(nested) if nested.path.is_ident("cfg_attr") => cfg_attr_sets_path(meta),
      | _ => false,
    })
  })
}
