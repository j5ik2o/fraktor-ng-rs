#[cfg(feature = "std")]
mod imp {
    pub struct Driver;
}

#[cfg(not(feature = "std"))]
mod imp {
    pub struct Driver;
}
