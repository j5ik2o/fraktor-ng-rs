use crate::{check_context::CheckContext, rule::Rule, sink::Sink, source_file::SourceFile};

// 未移植のスタブ。移植したら本文ごと置き換える。
pub(crate) struct Rustdoc;

impl Rule for Rustdoc {
    fn id(&self) -> &'static str {
        "rustdoc"
    }

    fn check(&self, _file: &SourceFile, _cx: &CheckContext<'_>, _sink: &mut Sink) {}
}
