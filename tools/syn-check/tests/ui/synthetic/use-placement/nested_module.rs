use std::fmt::Debug;

mod inner {
    fn helper() {}

    use std::fmt::Display;
}

impl Marker {
    fn method(&self) {
        use std::collections::HashMap;
    }
}

pub type Alias = u32;

extern crate alloc;

use std::fmt::Write;
