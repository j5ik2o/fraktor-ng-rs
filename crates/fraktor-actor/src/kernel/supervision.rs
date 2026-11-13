//! What a parent does with a child that fails.

mod directive;
mod failure_catcher;
mod supervisor_strategy;

pub(crate) use directive::Directive;
pub use failure_catcher::FailureCatcher;
pub use supervisor_strategy::SupervisorStrategy;
