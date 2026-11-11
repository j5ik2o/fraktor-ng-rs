// syn-check: allow(ambiguous-suffix) -- 外部 API の名前
pub struct ExternalManager;

pub struct OtherManager;

pub struct Config {
    // syn-check: allow(ambiguous-suffix) -- 外部形式のキー
    pub cache_manager: usize,
    pub other_manager: usize,
}

// syn-check: allow(ambiguous-suffix) -- impl 全体を許可する
impl Config {
    pub fn run_service(&self, job_engine: usize) {
        let local_util = job_engine;
        drop(local_util);
    }
}

pub fn after_impl(request_runtime: usize) {
    drop(request_runtime);
}
