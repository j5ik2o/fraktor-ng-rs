#[cfg(feature = "std")]
pub struct Clock {
    started: std::time::Instant,
}

#[cfg(not(feature = "std"))]
pub struct Clock {
    ticks: u64,
}

pub enum ClockError {
    Stopped,
}
