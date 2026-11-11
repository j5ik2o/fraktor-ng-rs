#![deny(cfg_std_forbid)]
// syn-check: allow(cfg-std-forbid) -- 旧 UI の allow ケース。旧 `#[allow(cfg_std_forbid)]` の置き換え
#[allow(cfg_std_forbid)]
#[cfg(feature = "std")]
fn allowed_fn() {}

fn main() {}
