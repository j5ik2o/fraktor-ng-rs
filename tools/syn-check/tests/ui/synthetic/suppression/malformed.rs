// syn-check: allow(type-per-file)
pub struct First;

// syn-check: allow(no-such-rule) -- 理由はある
pub struct Second;

// syn-check: deny(type-per-file) -- 形が違う
pub struct Third;

/// syn-check: allow(type-per-file) -- doc コメントは指定として扱わない
pub struct Fourth;
