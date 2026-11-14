//! A handle to an actor system that does not keep it alive.

use alloc::rc::Rc;
use alloc::rc::Weak;
use core::cell::RefCell;
use core::time::Duration;

use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::Executor;
use crate::kernel::message::Message;
use crate::kernel::supervision::SupervisorStrategy;

use crate::kernel::address::SystemName;

/// A handle to an actor system that does not keep it alive.
///
/// Mirrors the way a test probe reaches the system it runs in: the probe must
/// not own the system, because the test owns it, yet the probe still creates
/// children, watches actors, and drives messages. The handle holds a `Weak`
/// back-edge, which the ownership model permits. Calling it after the system
/// is gone panics, so a probe used after its spec has been torn down fails
/// loudly instead of silently doing nothing.
///
/// Once the system has been terminated, `spawn_child` and `function_ref` panic
/// with the same cause as `ActorSystem::actor_of`, because the executor owns
/// that state. Pekko's `attachChild` refuses a defunct guardian for the same
/// reason (`pekko/actor/.../dungeon/Children.scala:313-314`).
pub struct WeakActorSystem {
  name: SystemName,
  executor: Weak<RefCell<Executor>>,
}

impl WeakActorSystem {
  pub(crate) fn new(name: SystemName, executor: Weak<RefCell<Executor>>) -> Self {
    Self { name, executor }
  }

  fn upgrade(&self) -> Rc<RefCell<Executor>> {
    self.executor.upgrade().unwrap_or_else(|| panic!("the actor system {} is gone", self.name.as_str()))
  }

  /// Starts a child of `parent`.
  pub fn spawn_child(
    &self,
    parent: &ActorRef,
    props: Props,
    name: Option<&str>,
    strategy: SupervisorStrategy,
  ) -> ActorRef {
    self.upgrade().borrow_mut().spawn_child(parent, props, name, strategy)
  }

  /// Starts a handle whose shard runs `handler` for each delivered message.
  pub fn function_ref<H>(&self, name: &str, handler: H) -> ActorRef
  where
    H: Fn(&ActorRef, Message) + Send + Sync + 'static,
  {
    self.upgrade().borrow_mut().function_ref(name, handler)
  }

  /// Registers `watcher` to receive the `Terminated` notice of `target`.
  pub fn watch(&self, watcher: &ActorRef, target: &ActorRef) {
    self.upgrade().borrow_mut().watch(watcher, target);
  }

  /// Delivers at most one message and returns whether it delivered one.
  pub fn run_one(&self) -> bool {
    self.upgrade().borrow_mut().run_one()
  }

  /// Waits at most `max` for one message from the boundary and moves it onto
  /// the local queue. Returns whether a message arrived.
  pub fn await_envelope(&self, max: Duration) -> bool {
    self.upgrade().borrow_mut().await_envelope(max)
  }
}
