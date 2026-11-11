#![feature(register_tool)]
#![register_tool(module_wiring)]
#![warn(module_wiring::no_parent_reexport)]

pub mod child {
  pub mod sub {
    pub struct Thing;
    pub struct Other;
  }
}

// syn-check: allow(module-wiring) -- 旧 UI の許可ケース（直前の行のコメント）
pub use child::sub::Thing;
// syn-check: allow(module-wiring) -- 旧 UI の許可ケース（行末のコメント）
pub use child::sub::Other; // allow module_wiring::no_parent_reexport

fn main() {}
