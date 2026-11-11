// syn-check: allow(module-wiring) -- 旧 API の公開範囲を保つ
pub mod facade {
    pub mod leaf {}
}

pub mod exposed {}
