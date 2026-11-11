// syn-check: allow(rustdoc) -- 旧ドキュメントを移行するまでの暫定
/// 抑制された日本語の説明。
pub struct Allowed;

/// 抑制されていない日本語の説明。
pub struct NotAllowed;

// syn-check: allow(rustdoc) -- 本体の中の doc にもかかる
impl Allowed {
    /// 本体の中の日本語。
    pub fn method(&self) {}
}

// syn-check: allow(type-per-file) -- 別のルールの抑制は rustdoc に効かない
/// 別のルールを抑制しても対象のまま。
pub struct WrongRule;

// syn-check: allow(rustdoc) -- ignore フェンスにも効く
/// ```ignore
/// let value = 1;
/// ```
pub fn ignored_fence() {}
