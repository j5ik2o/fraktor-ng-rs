#![cfg_attr(not(test), no_std)]
#![deny(cfg_std_forbid)]

mod branches;
mod clock;
#[cfg(test)]
mod tests;
