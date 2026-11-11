pub mod inner_util {
    pub union RawEngine {
        pub raw: u8,
    }
}

pub trait JobEngine = Send;

pub fn outer() {
    struct LocalManager;
    type LocalService = u8;
    fn nested_engine() {}
    macro_rules! make_util {
        () => {};
    }
    make_util!();
}
