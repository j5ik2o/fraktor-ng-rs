use crate::{check_context::CheckContext, sink::Sink, source_file::SourceFile};

/// One structure rule. The checker calls `check` once per file.
pub trait Rule {
  /// The id used on the command line, in the JSON output, and in
  /// suppression comments. Kebab case, for example `type-per-file`.
  fn id(&self) -> &'static str;

  /// Whether the rule runs when the command line does not select rules.
  /// Experimental rules return `false`.
  fn default_enabled(&self) -> bool {
    true
  }

  fn check(&self, file: &SourceFile, cx: &CheckContext<'_>, sink: &mut Sink);
}
