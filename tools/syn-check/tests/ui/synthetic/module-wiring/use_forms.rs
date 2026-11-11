mod child {
    pub struct Thing;
    pub struct Other;
}

mod branch {
    mod leaf {}
}

use branch::*;
pub use child::{
    Thing,
    self,
};
pub use child::Other as _;
pub use ::core::fmt;
pub use core::fmt::Debug;
pub(self) use branch::*;
pub(crate) use child::Thing as Thing;

fn body() {
    pub use branch::leaf;
}
