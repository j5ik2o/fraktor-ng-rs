#![feature(register_tool)]
#![register_tool(type_per_file)]
#![warn(type_per_file::multiple_type_definitions)]

pub struct BaseType;
// syn-check: allow(type-per-file) -- 旧 UI ケース。旧 lint では下の #[allow] が 2 つ目の型を許可していた
#[allow(multiple_type_definitions)]
#[allow(type_per_file::multiple_type_definitions)]
pub enum AllowedEnum {
  Case,
}

fn main() {}
