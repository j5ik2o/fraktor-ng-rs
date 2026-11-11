/// Example in English.
///
/// ```rust
/// // 日本語のコメントは、フェンスの中なので許可される。
/// let value = 1;
/// ```
pub fn inside_fence() {}

/// ```rust
/// let value = 1;
/// ```
/// 閉じたあとの日本語。
pub fn after_fence() {}

/// ```rust
/// 閉じていないフェンスの中。
pub fn unclosed() {}

/// 前のブロックのフェンスは持ち越されない。
pub fn state_resets() {}

/// ```text
/// 日本語 in a text fence.
/// ```
pub fn other_fence_info() {}

/// ~~~
/// チルダのフェンスは数えないので、ここは対象。
/// ~~~
pub fn tilde_is_not_a_fence() {}

/// Indented fence:
///     ```rust
/// 日本語 inside an indented fence.
///     ```
pub fn indented_fence() {}

/// ```rust
/// let first = 1;
/// ```
/// Between the fences.
/// 二つ目のフェンスの前の日本語。
/// ```rust
/// let second = 2;
/// ```
pub fn two_fences() {}
