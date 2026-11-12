//! Cases for `module-wiring` that the old UI fixtures do not cover. The old
//! fixtures are single files with inline modules; these split the module
//! tree across files.

use std::{
  fs,
  path::{Path, PathBuf},
};

use syn_check::{layout::Layout, options::Options, report::Report};

const FIXTURES: &str = "tests/ui/synthetic/module-wiring";

fn run(root: &Path, layout: Layout) -> Report {
  let options = Options { layout, rules: Some(vec!["module-wiring".to_string()]) };
  let report = syn_check::run(&[root.to_path_buf()], &options).expect("run the checker");
  assert!(report.parse_failures.is_empty(), "{:?}", report.parse_failures);
  report
}

/// The violations as (path under `base`, line).
fn locations(report: &Report, base: &Path) -> Vec<(String, usize)> {
  report
    .violations
    .iter()
    .map(|violation| {
      let path = violation.path.strip_prefix(base).unwrap_or(&violation.path);
      (path.to_string_lossy().replace('\\', "/"), violation.position.line)
    })
    .collect()
}

fn violations(fixture: &str) -> Vec<(String, usize)> {
  let base = Path::new(FIXTURES);
  locations(&run(&base.join(fixture), Layout::Ng), base)
}

fn expected(locations: &[(&str, usize)]) -> Vec<(String, usize)> {
  locations.iter().map(|(path, line)| (path.to_string(), *line)).collect()
}

#[test]
fn leafness_of_file_modules_comes_from_their_files() {
  assert_eq!(
    violations("file_tree"),
    expected(&[
      // `branch.rs` は非 mod-rs なので、`pub mod exposed;` は `branch/exposed.rs` を指す。
      ("file_tree/src/branch.rs", 2),
      ("file_tree/src/lib.rs", 4),
      ("file_tree/src/lib.rs", 6),
    ])
  );
}

#[test]
fn modules_declared_through_inline_modules_resolve_their_children() {
  assert_eq!(
    violations("inline_chain"),
    expected(&[("inline_chain/src/lib.rs", 6), ("inline_chain/src/outer/middle.rs", 1),])
  );
}

#[test]
fn undeclared_files_are_crate_roots() {
  assert_eq!(
    violations("crate_roots"),
    expected(&[
      // クレートのルートは prelude という名前のモジュールではない。
      ("crate_roots/src/bin/prelude.rs", 5),
      // クレートのルートの子は同じディレクトリに置かれる。
      ("crate_roots/tests/it.rs", 1),
    ])
  );
}

#[test]
fn test_only_child_modules_do_not_make_a_branch() {
  assert_eq!(violations("test_hook"), expected(&[("test_hook/src/lib.rs", 4)]));
}

#[test]
fn every_leaf_of_a_use_tree_is_checked() {
  assert_eq!(
    violations("use_forms.rs"),
    expected(&[
      ("use_forms.rs", 13),
      ("use_forms.rs", 15),
      ("use_forms.rs", 16),
      ("use_forms.rs", 17),
      ("use_forms.rs", 18),
      ("use_forms.rs", 22),
    ])
  );
}

#[test]
fn modules_in_function_bodies_belong_to_the_enclosing_module() {
  assert_eq!(violations("body_items.rs"), expected(&[("body_items.rs", 9), ("body_items.rs", 12)]));
}

#[test]
fn include_is_found_by_the_old_text_scan() {
  assert_eq!(
    violations("include.rs"),
    expected(&[
      // 外側と内側のモジュールが同じ位置を見つけるが、1 件にまとめる。
      ("include.rs", 3),
      // 旧 lint と同じく、文字列リテラルの中も拾う。
      ("include.rs", 13),
    ])
  );
}

#[test]
fn path_attributes_are_reported() {
  assert_eq!(violations("path_attrs.rs"), expected(&[("path_attrs.rs", 2), ("path_attrs.rs", 4)]));
}

#[test]
fn sibling_test_hook_needs_cfg_test_on_the_line_before() {
  assert_eq!(violations("hook_with_comment.rs"), expected(&[("hook_with_comment.rs", 3)]));
  assert_eq!(violations("hook_any_cfg.rs"), expected(&[("hook_any_cfg.rs", 2)]));
}

#[test]
fn a_module_without_a_file_is_not_judged() {
  assert_eq!(violations("missing_file.rs"), expected(&[]));
}

#[test]
fn allow_covers_the_modules_inside_the_next_module() {
  let report = run(&Path::new(FIXTURES).join("suppression/allow_inline_module.rs"), Layout::Ng);
  assert_eq!(locations(&report, Path::new(FIXTURES)), expected(&[("suppression/allow_inline_module.rs", 6)]));
  assert!(report.suppressions[0].used);
}

#[test]
fn allow_file_covers_the_file() {
  let report = run(&Path::new(FIXTURES).join("suppression/allow_file.rs"), Layout::Ng);
  assert_eq!(locations(&report, Path::new(FIXTURES)), expected(&[]));
  assert!(report.suppressions[0].file_scope && report.suppressions[0].used);
}

#[test]
fn the_layout_does_not_change_the_result() {
  let base = Path::new(FIXTURES);
  for fixture in ["file_tree", "crate_roots", "use_forms.rs"] {
    assert_eq!(
      locations(&run(&base.join(fixture), Layout::Legacy), base),
      locations(&run(&base.join(fixture), Layout::Ng), base),
      "{fixture}"
    );
  }
}

/// A tree with `mod.rs` files. The repository forbids `mod.rs`, so the tree
/// is written to a temporary directory instead of being committed.
#[test]
fn mod_rs_files_resolve_their_children_in_their_own_directory() {
  let root = std::env::temp_dir().join(format!("syn-check-module-wiring-mod-rs-{}", std::process::id()));
  let files = [
    ("src/lib.rs", "mod legacy;\npub mod dup;\n\npub use legacy::Legacy;\n"),
    ("src/legacy/mod.rs", "mod part;\npub mod open;\n\npub use part::Legacy;\n"),
    ("src/legacy/part.rs", "pub struct Legacy;\n"),
    ("src/legacy/open.rs", "pub struct Open;\n"),
    // `dup.rs` と `dup/mod.rs` が両方あるので、rustc はどちらも選ばない。
    ("src/dup.rs", "pub struct Dup;\n"),
    ("src/dup/mod.rs", "pub struct Dup;\n"),
  ];
  if root.exists() {
    fs::remove_dir_all(&root).expect("remove fixtures left by an earlier run");
  }
  for (path, text) in files {
    let path: PathBuf = root.join(path);
    fs::create_dir_all(path.parent().expect("has a directory")).expect("create a directory");
    fs::write(path, text).expect("write a fixture");
  }
  let report = run(&root, Layout::Ng);
  fs::remove_dir_all(&root).expect("remove the fixtures");
  assert_eq!(locations(&report, &root), expected(&[("src/legacy/mod.rs", 2), ("src/lib.rs", 4)]));
}
