// syn-check: allow-file(mod-file) -- 旧 UI ケース。旧 lint では取り込み側の #![allow(no_mod_rs)] が許可していた
pub fn value() -> &'static str {
  "ok"
}
