pub struct First;

// syn-check: allow(type-per-file) -- 2 つ目の型は First とだけ一緒に使う
#[derive(Debug)]
pub struct Second {
    value: u32,
}

pub struct Third;
