// スパイクが偽陽性として報告した形：cfg で切り替える同名の type alias（旧 utils-core/src/sync.rs 相当）。
#[cfg(feature = "std")]
pub type DefaultMutex<T> = std::sync::Mutex<T>;
#[cfg(not(feature = "std"))]
pub type DefaultMutex<T> = spin::Mutex<T>;

#[cfg(feature = "std")]
pub type DefaultRwLock<T> = std::sync::RwLock<T>;
#[cfg(not(feature = "std"))]
pub type DefaultRwLock<T> = spin::RwLock<T>;

#[cfg(feature = "std")]
pub type DefaultOnce<T> = std::sync::OnceLock<T>;
#[cfg(not(feature = "std"))]
pub type DefaultOnce<T> = spin::Once<T>;

pub struct SyncGuard;
