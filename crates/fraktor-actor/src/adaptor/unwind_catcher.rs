//! A failure catcher backed by `catch_unwind`.

use core::panic::AssertUnwindSafe;

use crate::kernel::supervision::FailureCatcher;

/// Catches the panic a failing actor raises.
///
/// Pekko turns the exception of a failing actor into a supervisor decision.
/// The port models a failure as a panic, because a Rust handler has no return
/// value to carry a cause, and this adapter is what stops the panic from
/// unwinding through the executor. It lives in the adaptor because the kernel
/// cannot use the standard library.
pub struct UnwindCatcher;

impl FailureCatcher for UnwindCatcher {
  fn run(&self, body: &mut dyn FnMut()) -> bool {
    std::panic::catch_unwind(AssertUnwindSafe(body)).is_ok()
  }
}
