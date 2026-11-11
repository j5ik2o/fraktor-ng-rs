/// First line is English.
/// 二行目が日本語。
/// 三行目も日本語だが、同じブロックでは最初の 1 件だけ報告する。
pub fn one_report_per_block() {}

/// 最初のブロック。

/// 空行で分かれた次のブロック。
pub fn blank_line_splits() {}

/// 最初のブロック。
// 通常のコメントはブロックを区切る。
/// 次のブロック。
pub fn plain_comment_splits() {}

/// English only.
/// 日本語はここ。
#[derive(Clone)]
/// 属性をはさむと別のブロックになる。
pub struct AttributeSplits;
