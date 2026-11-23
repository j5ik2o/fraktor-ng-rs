//! What a parent does with a child that fails.

mod directive;
mod failure_catcher;
mod failure_kind;
mod restart_history;
mod restart_limit;
mod run_outcome;
mod supervisor_strategy;

pub use directive::Directive;
pub use failure_catcher::FailureCatcher;
pub use failure_kind::FailureKind;

pub(crate) use restart_history::RestartHistory;
pub(crate) use restart_limit::RestartLimit;

pub use run_outcome::RunOutcome;
pub use supervisor_strategy::SupervisorStrategy;
