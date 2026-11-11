use crate::{check_context::CheckContext, rule::Rule, sink::Sink, source_file::SourceFile};

// 未移植のスタブ。移植したら本文ごと置き換える。
pub(crate) struct PortAdaptorBoundary;

impl Rule for PortAdaptorBoundary {
    fn id(&self) -> &'static str {
        "port-adaptor-boundary"
    }

    fn check(&self, _file: &SourceFile, _cx: &CheckContext<'_>, _sink: &mut Sink) {}
}
