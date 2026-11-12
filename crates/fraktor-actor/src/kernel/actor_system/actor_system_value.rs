//! An actor system and its lifecycle.

use alloc::sync::Arc;
use core::fmt;
use core::time::Duration;

use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::Executor;
use crate::kernel::executor::ShardBoundary;

#[cfg(test)]
#[path = "actor_system_value_test.rs"]
mod tests;

/// The root of an actor hierarchy, named by the caller at creation.
///
/// Mirrors `org.apache.pekko.actor.ActorSystem` for the lifecycle that G0b
/// needs: creating a system by name on a given shard boundary, starting actors
/// with `actor_of`, driving their messages to completion, and terminating the
/// system. Dispatchers, guardians, and extensions belong to later goals.
pub struct ActorSystem {
  name: String,
  executor: Executor,
  terminated: bool,
}

impl ActorSystem {
  /// Creates a running actor system with the given name, whose actors share
  /// `boundary` as the edge of their shard.
  pub fn new(name: &str, boundary: Arc<dyn ShardBoundary>) -> Self {
    Self { name: name.to_string(), executor: Executor::new(boundary), terminated: false }
  }

  /// Returns the name the system was created with.
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Starts the actor a recipe describes and returns a handle to it.
  ///
  /// Each call hands out a handle with a fresh identifier and generation zero,
  /// so handles of different actors compare unequal.
  pub fn actor_of(&mut self, props: Props) -> ActorRef {
    self.executor.spawn(props)
  }

  /// Delivers every message that is waiting until nothing is left.
  pub fn run_until_idle(&mut self) {
    self.executor.run_until_idle();
  }

  /// Waits at most `max` for one message from the boundary and moves it onto
  /// the local queue without delivering it; call `run_until_idle` to deliver
  /// it. Returns whether a message arrived.
  pub fn await_envelope(&mut self, max: Duration) -> bool {
    self.executor.await_envelope(max)
  }

  /// Stops the system. Calling it again is a no-op, as in Pekko.
  pub fn terminate(&mut self) {
    if !self.terminated {
      self.executor.stop_all();
      self.terminated = true;
    }
  }

  /// Returns whether the system has been terminated.
  pub fn is_terminated(&self) -> bool {
    self.terminated
  }
}

impl fmt::Debug for ActorSystem {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.debug_struct("ActorSystem").field("name", &self.name).field("terminated", &self.terminated).finish()
  }
}
