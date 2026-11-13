//! A failure catcher backed by `catch_unwind`.

use alloc::string::String;
use core::any::Any;
use core::panic::AssertUnwindSafe;

use crate::kernel::supervision::FailureCatcher;
use crate::kernel::supervision::RunOutcome;

/// The message reported when a panic carries no string payload.
const UNKNOWN_FAILURE_MESSAGE: &str = "";

/// Catches the panic a failing actor raises.
///
/// Pekko turns the exception of a failing actor into a supervisor decision.
/// The port models a failure as a panic, because a Rust handler has no return
/// value to carry a cause, and this adapter is what stops the panic from
/// unwinding through the executor. It lives in the adaptor because the kernel
/// cannot use the standard library.
///
/// The failure message is the payload of the panic: `&'static str` and `String`
/// are read as text, which is how a ported test states the cause it expects. A
/// panic with any other payload reports an empty message.
pub struct UnwindCatcher;

impl FailureCatcher for UnwindCatcher {
  fn run(&self, body: &mut dyn FnMut()) -> RunOutcome {
    match std::panic::catch_unwind(AssertUnwindSafe(body)) {
      | Ok(()) => RunOutcome::Completed,
      | Err(payload) => RunOutcome::Failed { message: panic_message(payload.as_ref()) },
    }
  }
}

/// Returns the text a panic payload carries, or an empty string.
fn panic_message(payload: &(dyn Any + Send)) -> String {
  if let Some(text) = payload.downcast_ref::<&'static str>() {
    return String::from(*text);
  }
  if let Some(text) = payload.downcast_ref::<String>() {
    return text.clone();
  }
  String::from(UNKNOWN_FAILURE_MESSAGE)
}
