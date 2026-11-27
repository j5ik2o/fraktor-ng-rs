// SPDX-License-Identifier: Apache-2.0

use std::{
  fs,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

use syn_check::pub_api;

struct TempCrate {
  root: PathBuf,
}

impl TempCrate {
  fn new(label: &str) -> Self {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let root = std::env::temp_dir().join(format!("syn-check-pub-api-{label}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(root.join("src")).expect("create src dir");
    Self { root }
  }

  fn write(&self, rel: &str, content: &str) {
    let path = self.root.join(rel);
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content).expect("write fixture");
  }

  fn path(&self) -> &Path {
    &self.root
  }
}

impl Drop for TempCrate {
  fn drop(&mut self) {
    let _removed = fs::remove_dir_all(&self.root);
  }
}

#[test]
fn extracts_reexported_leaf_types_and_methods_and_ignores_private_and_tests() {
  let tmp = TempCrate::new("wiring");
  tmp.write("Cargo.toml", "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\n");
  tmp.write("src/lib.rs", "pub mod kernel;\n#[cfg(test)]\npub mod test_only { pub struct Ignored; }\n");
  tmp.write(
    "src/kernel.rs",
    "mod internal;\nmod widget_value;\npub use widget_value::Widget;\npub(crate) use internal::Secret;\n",
  );
  tmp.write("src/kernel/internal.rs", "pub struct Secret;\nimpl Secret { pub fn hidden(&self) {} }\n");
  tmp.write(
    "src/kernel/widget_value.rs",
    "#[cfg(test)]\n#[path = \"widget_value_test.rs\"]\nmod tests;\npub struct Widget { pub count: usize, private_field: bool }\nimpl Widget {\n  pub fn create(name: &str) -> Self { Self { count: name.len(), private_field: false } }\n  pub(crate) fn internal_only(&self) {}\n}\n",
  );
  tmp.write("src/kernel/widget_value_test.rs", "pub fn helper_in_test() {}\n");

  let items = pub_api::collect_crate(tmp.path()).expect("collect");

  assert!(items.contains(&"pub mod demo_crate::kernel".to_string()), "{items:#?}");
  assert!(items.contains(&"pub struct demo_crate::kernel::Widget".to_string()), "{items:#?}");
  assert!(items.contains(&"pub demo_crate::kernel::Widget::count: usize".to_string()), "{items:#?}");
  assert!(items.contains(&"pub fn demo_crate::kernel::Widget::create(name: &str) -> Self".to_string()), "{items:#?}");
  assert!(!items.iter().any(|line| line.contains("Secret")), "pub(crate) leaked: {items:#?}");
  assert!(!items.iter().any(|line| line.contains("internal_only")), "pub(crate) method leaked: {items:#?}");
  assert!(!items.iter().any(|line| line.contains("private_field")), "private field leaked: {items:#?}");
  assert!(!items.iter().any(|line| line.contains("Ignored")), "cfg(test) leaked: {items:#?}");
  assert!(!items.iter().any(|line| line.contains("helper_in_test")), "test file leaked: {items:#?}");
}

#[test]
fn extracts_enums_traits_consts_and_macro_exports() {
  let tmp = TempCrate::new("items");
  tmp.write("Cargo.toml", "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n");
  tmp.write(
    "src/lib.rs",
    "#[macro_export]\nmacro_rules! my_macro { () => {}; }\npub const LIMIT: usize = 8;\npub enum Mode { Fast, Slow(u32) }\npub trait Runner { type Output; fn run(&self) -> Self::Output; }\n",
  );

  let items = pub_api::collect_crate(tmp.path()).expect("collect");

  assert!(items.contains(&"pub macro sample::my_macro!".to_string()), "{items:#?}");
  assert!(items.contains(&"pub const sample::LIMIT: usize".to_string()), "{items:#?}");
  assert!(items.contains(&"pub enum sample::Mode".to_string()), "{items:#?}");
  assert!(items.contains(&"pub sample::Mode::Fast".to_string()), "{items:#?}");
  assert!(items.contains(&"pub sample::Mode::Slow(u32)".to_string()), "{items:#?}");
  assert!(items.contains(&"pub trait sample::Runner".to_string()), "{items:#?}");
  assert!(items.contains(&"pub type sample::Runner::Output".to_string()), "{items:#?}");
  assert!(items.contains(&"pub fn sample::Runner::run(&self) -> Self::Output".to_string()), "{items:#?}");
}

#[test]
fn cyclic_pub_use_does_not_recurse_infinitely() {
  let tmp = TempCrate::new("cycle");
  tmp.write("Cargo.toml", "[package]\nname = \"cyc\"\nversion = \"0.1.0\"\n");
  tmp.write("src/lib.rs", "pub mod a;\npub mod b;\n");
  tmp.write("src/a.rs", "pub use crate::b::X;\n");
  tmp.write("src/b.rs", "pub use crate::a::X;\n");

  let items = pub_api::collect_crate(tmp.path()).expect("collect");
  assert!(items.contains(&"pub mod cyc::a".to_string()), "{items:#?}");
  assert!(items.contains(&"pub mod cyc::b".to_string()), "{items:#?}");
}

#[test]
fn methods_in_an_impl_in_another_module_belong_to_the_type() {
  let tmp = TempCrate::new("cross-impl");
  tmp.write("Cargo.toml", "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n");
  tmp.write("src/lib.rs", "pub mod kernel;\npub mod adaptor;\n");
  tmp.write("src/kernel.rs", "mod handle_value;\npub use handle_value::Handle;\npub struct Twin;\n");
  tmp.write("src/kernel/handle_value.rs", "pub struct Handle;\nimpl Handle { pub fn tell(&self) {} }\n");
  tmp.write(
    "src/adaptor.rs",
    "use crate::kernel::Handle;\nimpl Handle { pub fn ask(&self) {} }\nstruct Twin;\nimpl Twin { pub fn private_twin(&self) {} }\n",
  );

  let items = pub_api::collect_crate(tmp.path()).expect("collect");

  assert!(items.contains(&"pub fn demo::kernel::Handle::tell(&self)".to_string()), "{items:#?}");
  assert!(items.contains(&"pub fn demo::kernel::Handle::ask(&self)".to_string()), "{items:#?}");
  // Two types are named `Twin`, so an impl outside the defining module is not attributed.
  assert!(!items.iter().any(|line| line.contains("private_twin")), "{items:#?}");
}

#[test]
fn where_clauses_and_supertraits_are_part_of_the_line() {
  let tmp = TempCrate::new("bounds");
  tmp.write("Cargo.toml", "[package]\nname = \"bounds\"\nversion = \"0.1.0\"\n");
  tmp.write(
    "src/lib.rs",
    "pub fn make<F>(factory: F) -> u8 where F: Fn() -> u8 + Clone { factory() }\npub trait Sub: Clone + Send {}\npub struct Wrap<T>(pub T) where T: Copy;\n",
  );

  let items = pub_api::collect_crate(tmp.path()).expect("collect");

  assert!(
    items.contains(&"pub fn bounds::make<F>(factory: F) -> u8 where F: Fn() -> u8 + Clone".to_string()),
    "{items:#?}"
  );
  assert!(items.contains(&"pub trait bounds::Sub: Clone + Send".to_string()), "{items:#?}");
  assert!(items.contains(&"pub struct bounds::Wrap<T> where T: Copy".to_string()), "{items:#?}");
}

#[test]
fn a_module_reexported_inside_itself_does_not_recurse_infinitely() {
  let tmp = TempCrate::new("alias-cycle");
  tmp.write("Cargo.toml", "[package]\nname = \"loopy\"\nversion = \"0.1.0\"\n");
  tmp.write("src/lib.rs", "pub mod a;\n");
  tmp.write("src/a.rs", "pub mod b;\npub use crate::a as again;\npub struct X;\n");
  tmp.write("src/a/b.rs", "pub use super::*;\npub struct Y;\n");

  let items = pub_api::collect_crate(tmp.path()).expect("collect");

  assert!(items.contains(&"pub struct loopy::a::X".to_string()), "{items:#?}");
  assert!(items.contains(&"pub struct loopy::a::b::Y".to_string()), "{items:#?}");
}
