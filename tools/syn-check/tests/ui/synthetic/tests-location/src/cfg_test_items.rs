//! find_cfg_test_items: cfg(test) 属性のうち、sibling test hook と tests モジュールの宣言以外はすべて違反。

#[cfg(test)]
use std::fmt;

#[cfg(test)]
fn helper() {}

pub struct Probe {
    #[cfg(test)]
    count: usize,
}

impl Probe {
    #[cfg(test)]
    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests_util;

#[cfg(test)]
#[allow(dead_code)]
mod tests {}
