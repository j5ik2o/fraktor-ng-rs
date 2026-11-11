#![feature(register_tool)]
#![register_tool(ambiguous_suffix)]
#![warn(ambiguous_suffix::ambiguous_suffix)]

pub struct NormalRegistry;
// syn-check: allow(ambiguous-suffix) -- 旧 UI ケース。旧 lint では下の #[allow] が外部 API 由来の名前を許可していた
#[allow(ambiguous_suffix)]
#[allow(ambiguous_suffix::ambiguous_suffix)]
pub struct ExternalApiManager;

fn main() {}
