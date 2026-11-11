/// ```ignore
/// let value = 1;
/// ```
pub fn plain() {}

/// ```rust,ignore
/// let value = 1;
/// ```
pub fn after_comma() {}

/// ```rust ignore
/// let value = 1;
/// ```
pub fn after_space() {}

/// ```IGNORE
/// let value = 1;
/// ```
pub fn upper_case() {}

/// ```ignore,rust
/// let value = 1;
/// ```
pub fn before_comma() {}

/// ```rust, ignore
/// let value = 1;
/// ```
pub fn comma_then_space() {}

/// ```ignored
/// let value = 1;
/// ```
pub fn longer_word() {}

/// ```ignore-test
/// let value = 1;
/// ```
pub fn hyphenated() {}

/// ````ignore
/// let value = 1;
/// ````
pub fn four_backticks() {}

/// ```no_run
/// let value = 1;
/// ```
pub fn no_run() {}

/// ```should_panic,compile_fail
/// let value = 1;
/// ```
pub fn other_flags() {}

/// ```rust
/// let value = 1;
/// ```ignore
pub fn closing_fence_with_info() {}

/// ```rust
/// let first = 1;
/// ```
/// ```ignore
/// let second = 2;
/// ```
pub fn second_fence() {}

mod inner {
    //! ```ignore
    //! let value = 1;
    //! ```
    pub fn inner_doc() {}
}
