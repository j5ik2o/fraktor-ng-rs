use std::path::{Component, Path};

use crate::{
  check_context::CheckContext, layout::Layout, position::Position, rule::Rule, sink::Sink, source_file::SourceFile,
};

/// Forbids runnable examples inside a crate's `examples` directory.
/// Ported from the old `module-examples-lint` (`module_examples_forbid`).
pub(crate) struct ModuleExamples;

impl Rule for ModuleExamples {
  fn id(&self) -> &'static str {
    "module-examples"
  }

  fn check(&self, file: &SourceFile, cx: &CheckContext<'_>, sink: &mut Sink) {
    let (crates_dir, message) = match cx.layout {
      | Layout::Legacy => (
        "modules",
        "modules/**/examples 配下に runnable example を置いてはいけません。example は `showcases/std/<example-name>/main.rs` へ移動してください",
      ),
      // 暫定。移動先の置き場所は ng ではまだ決まっていないので、移動先は名指ししない。
      | Layout::Ng => (
        "crates",
        "crates/**/examples 配下に runnable example を置いてはいけません。example を別の置き場所へ移動してください（ng の置き場所は未定）",
      ),
    };
    if !is_example_file(file.path(), crates_dir) {
      return;
    }
    // 旧 lint はアイテムを訪れたときに報告した。アイテムの無いファイルには出さない。
    let Some(ast) = file.ast() else {
      return;
    };
    if ast.items.is_empty() {
      return;
    }
    sink.report(Position::START, message);
  }
}

/// `<crates_dir>/<crate>/examples/<file>` anywhere in the path.
/// The old lint tested exactly four consecutive path components, so a file in
/// a subdirectory of `examples` also matches.
fn is_example_file(path: &Path, crates_dir: &str) -> bool {
  if path.extension().is_none_or(|extension| extension != "rs") {
    return false;
  }

  if path.components().any(|component| component.as_os_str() == "target") {
    return false;
  }

  let components: Vec<_> = path.components().collect();
  components.windows(4).any(|window| {
    matches!(
        window,
        [
            Component::Normal(first),
            Component::Normal(_crate_name),
            Component::Normal(examples),
            Component::Normal(_file_name),
        ] if *first == crates_dir && *examples == "examples"
    )
  })
}
