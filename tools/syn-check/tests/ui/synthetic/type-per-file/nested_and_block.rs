pub struct Outer;

mod inner {
    pub trait Nested {}
}

fn build() {
    pub enum Local {
        Only,
    }
}
