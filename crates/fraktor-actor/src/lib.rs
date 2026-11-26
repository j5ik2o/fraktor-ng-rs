//! Actor core for fraktor-ng-rs.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "std")]
pub mod adaptor;
pub mod kernel;
