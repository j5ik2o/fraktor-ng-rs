//! What a parent does with a child that fails.

mod directive;
mod failure_catcher;
mod run_outcome;
mod supervisor_strategy;

pub(crate) use directive::Directive;
pub use failure_catcher::FailureCatcher;
pub use run_outcome::RunOutcome;
pub(crate) use supervisor_strategy::FailureKind;
pub use supervisor_strategy::SupervisorStrategy;
