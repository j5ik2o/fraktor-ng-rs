use core::fmt;

// syn-check: allow(cfg-std-forbid) -- 段階2 で adaptor へ移すまでの暫定
#[cfg(feature = "std")]
mod std_bridge {
  use std::sync::Mutex;
}

use std::thread;
