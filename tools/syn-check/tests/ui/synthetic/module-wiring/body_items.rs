mod with_fn {
    pub struct Shown;

    fn hidden() {
        mod nested {}
    }
}

pub use with_fn::Shown;

fn host() {
    pub mod in_body {}
}
