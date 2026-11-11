//! 旧実装はソーステキストを走査するので、コメントと文字列の中も拾う。
//!
//! ```
//! #[test]
//! fn in_doc() {}
//! ```

// mod tests { }
/* #[cfg(test)] */
const PATTERN: &str = "#[cfg(test)] mod tests {";

// 属性の本体の中は読み飛ばすので、次の doc 属性の中の test 属性は拾わない。
#[doc = "#[test]"]
fn documented() {}
