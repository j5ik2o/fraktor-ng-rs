use std::path::Path;

use crate::{check_context::CheckContext, position::Position, rule::Rule, sink::Sink, source_file::SourceFile};

/// Forbids `mod.rs`. Ported from the old `mod-file-lint` (`no_mod_rs`).
pub(crate) struct ModFile;

impl Rule for ModFile {
  fn id(&self) -> &'static str {
    "mod-file"
  }

  fn check(&self, file: &SourceFile, _cx: &CheckContext<'_>, sink: &mut Sink) {
    if is_mod_rs(file.path()) {
      sink.report(
                Position::START,
                "mod.rs は使用禁止です。`<module>.rs` にリネームし、サブモジュールは同じディレクトリの `<module>/` に置いてください",
            );
    }
  }
}

fn is_mod_rs(path: &Path) -> bool {
  if path.components().any(|component| component.as_os_str() == "target") {
    return false;
  }
  path.file_name().is_some_and(|name| name == "mod.rs")
}
