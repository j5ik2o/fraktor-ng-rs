unsafe extern "C" {
    pub fn open_manager(path_service: usize, callback: fn(inner_engine: usize));
    pub static LEGACY_ENGINE: u8;
}

pub type Callback = fn(usize, _: u8);
