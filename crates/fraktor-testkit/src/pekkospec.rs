//! A base for tests that need an actor system.

mod pekko_spec;
mod pekko_spec_hooks;

pub use pekko_spec::PekkoSpec;
pub use pekko_spec_hooks::PekkoSpecHooks;
