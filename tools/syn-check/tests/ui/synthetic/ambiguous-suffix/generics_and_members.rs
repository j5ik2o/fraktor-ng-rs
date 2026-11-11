pub struct Holder<'job_service, SlotRuntime, const SLOT_UTIL: usize> {
    pub cache_manager: &'job_service SlotRuntime,
    pub pair: (usize, usize),
}

pub struct Pair(pub usize, pub usize);

pub struct Quiet<'a, T: 'static, const N: usize>(&'a T);

pub trait Source {
    type OutputEngine;
    const LIMIT_FACADE: u8;
    fn run_service(&self);
}

pub fn higher(_call: for<'call_manager> fn(&'call_manager str)) {}
