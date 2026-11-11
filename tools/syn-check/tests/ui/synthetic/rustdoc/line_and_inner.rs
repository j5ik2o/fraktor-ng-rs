//! 日本語のモジュール説明。
//! The second line is English.

/// Item doc in English.
pub struct Clean;

/// 日本語の説明。
pub struct Japanese;

    /// インデントされた日本語。
fn indented() {}

/// Accent é before the Japanese: 日本語。
pub struct Columns;

mod inner {
    //! 内側の説明。
    /// 外側の説明。
    pub fn both() {}
}
