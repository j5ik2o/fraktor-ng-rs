// cfg で排他になる同名の公開型は 1 つとして数える。
#[cfg(feature = "std")]
pub struct Clock {
    started: std::time::Instant,
}

#[cfg(not(feature = "std"))]
pub struct Clock {
    ticks: u64,
}
