//! An actor system and its lifecycle.

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use core::fmt;
use core::time::Duration;

use crate::kernel::actor::Props;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_path::RootActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::actor_system::weak_actor_system::WeakActorSystem;
use crate::kernel::address::Address;
use crate::kernel::address::SystemName;
use crate::kernel::executor::Executor;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::supervision::FailureCatcher;

#[cfg(test)]
#[path = "actor_system_value_test.rs"]
mod tests;

/// The protocol of the address a local ported actor system answers on.
const LOCAL_PROTOCOL: &str = "pekko";

/// The root of an actor hierarchy, named by the caller at creation.
///
/// Mirrors `org.apache.pekko.actor.ActorSystem` for the lifecycle the ported
/// tests need: creating a system by name on a given shard boundary, starting
/// actors with `actor_of`, driving their messages to completion, stopping an
/// actor, and terminating the system. Dispatchers, guardians, and extensions
/// belong to later goals.
///
/// The executor sits behind an `Rc<RefCell<..>>` so that a [`WeakActorSystem`]
/// can reach it after the guardian is gone. A `Weak` is the only shared
/// pointer here; the executor stays singly owned by the system while the
/// system is alive.
///
/// Whether the system has been terminated is kept by the executor, so a handle
/// that reaches the system by its weak reference is refused in exactly the same
/// way as this value.
pub struct ActorSystem {
  name: SystemName,
  executor: Rc<RefCell<Executor>>,
}

impl ActorSystem {
  /// Creates a running actor system with the given name, whose actors share
  /// `boundary` as the edge of their shard.
  ///
  /// # Panics
  ///
  /// Panics when `name` is empty.
  pub fn new(name: &str, boundary: Arc<dyn ShardBoundary>) -> Self {
    Self::try_new(name, boundary).unwrap_or_else(|message| panic!("{message}"))
  }

  /// Creates a running actor system with the given name, or reports the
  /// violated constraint instead of panicking.
  pub fn try_new(name: &str, boundary: Arc<dyn ShardBoundary>) -> Result<Self, String> {
    let name = SystemName::new(name)?;
    let executor = Rc::new(RefCell::new(Executor::new(Self::root_path(name.as_str()), boundary)));
    Ok(Self { name, executor })
  }

  /// Creates a running actor system that reports a failing actor to `catcher`.
  ///
  /// The catcher is what lets a parent apply a supervisor strategy: the kernel
  /// cannot catch a panic, so it asks the port. A system created with `new`
  /// has no catcher and lets a panic out, which keeps the behaviour of the
  /// earlier milestones.
  pub fn with_failure_catcher(name: &str, boundary: Arc<dyn ShardBoundary>, catcher: Arc<dyn FailureCatcher>) -> Self {
    let system = Self::new(name, boundary);
    system.executor.borrow_mut().set_failure_catcher(catcher);
    system
  }

  /// Installs `sink`, which receives the log events the system raises.
  ///
  /// Mirrors the role of Pekko's event stream for the failures of a faulty
  /// actor: a test attaches a sink to watch the `Error` events of a stopped or
  /// restarted child. A system created without a sink drops the events.
  pub fn set_event_sink(&self, sink: Arc<dyn crate::kernel::actor::Logging::EventSink>) {
    self.executor.borrow_mut().set_event_sink(sink);
  }

  fn root_path(name: &str) -> ActorPath {
    let root = RootActorPath::new(Address::new(LOCAL_PROTOCOL, name));
    ActorPath::from_root(root)
  }

  /// Returns the name the system was created with.
  pub fn name(&self) -> &str {
    self.name.as_str()
  }

  /// Starts the actor a recipe describes and returns a handle to it.
  ///
  /// Each call hands out a handle with a fresh identifier and generation zero,
  /// so handles of different actors compare unequal. The actor is started below
  /// `/user`, with the default supervisor strategy, as the guardian does for a
  /// top-level actor.
  ///
  /// Panics once the system has been terminated, with the same cause as Pekko,
  /// which throws `IllegalStateException` from `attachChild` when a defunct
  /// guardian cannot create children.
  pub fn actor_of(&mut self, props: Props) -> ActorRef {
    self.executor.borrow_mut().spawn_user(props)
  }

  /// Stops `target`, telling its watchers that it is gone.
  ///
  /// Mirrors `org.apache.pekko.actor.ActorSystem.stop`, which asks the
  /// guardian to stop the actor.
  pub fn stop(&mut self, target: &ActorRef) {
    self.executor.borrow_mut().stop(target);
  }

  /// Registers `watcher` to receive the `Terminated` notice of `target`.
  ///
  /// Mirrors `ActorContext.watch` with the scope of the system's guardian. The
  /// scope is the actors of this system and the function references that this
  /// system made for its test actors. A target outside that scope, such as an
  /// actor of another system, a reference that has already stopped, or the
  /// reply address of an `ask`, is reported at once with `existence_confirmed =
  /// false`; no later notice arrives for it, even if it stops afterwards.
  pub fn watch(&mut self, watcher: &ActorRef, target: &ActorRef) {
    self.executor.borrow_mut().watch(watcher, target);
  }

  /// Starts a handle whose shard runs `handler` for each delivered message.
  ///
  /// This is how the port expresses Pekko's test actor, which runs on the
  /// calling-thread dispatcher: the handle shares no cell, and posting to it
  /// runs the handler before the sender returns. The path is
  /// `<root>/system/<name>`, mirroring `systemActorOf`.
  ///
  /// Panics once the system has been terminated, as `actor_of` does: Pekko's
  /// `systemActorOf` asks the guardian for a child, so a defunct system refuses
  /// it with the same cause.
  pub fn function_ref<H>(&mut self, name: &str, handler: H) -> ActorRef
  where
    H: Fn(&ActorRef, crate::kernel::message::Message) + Send + Sync + 'static,
  {
    self.executor.borrow_mut().function_ref(name, handler)
  }

  /// Returns a handle that can reach the executor without owning the system.
  pub fn downgrade(&self) -> WeakActorSystem {
    WeakActorSystem::new(self.name.clone(), Rc::downgrade(&self.executor))
  }

  /// Delivers every message that is waiting until nothing is left.
  pub fn run_until_idle(&mut self) {
    self.executor.borrow_mut().run_until_idle();
  }

  /// Delivers at most one message and returns whether it delivered one, so a
  /// caller can check a deadline between messages instead of waiting for the
  /// whole queue to drain.
  pub fn run_one(&mut self) -> bool {
    self.executor.borrow_mut().run_one()
  }

  /// Waits at most `max` for one message from the boundary and moves it onto
  /// the local queue without delivering it; call `run_until_idle` to deliver
  /// it. Returns whether a message arrived.
  pub fn await_envelope(&mut self, max: Duration) -> bool {
    self.executor.borrow_mut().await_envelope(max)
  }

  /// Stops the system. Calling it again is a no-op, as in Pekko.
  pub fn terminate(&mut self) {
    if !self.is_terminated() {
      self.executor.borrow_mut().stop_all();
    }
  }

  /// Returns whether the system has been terminated.
  pub fn is_terminated(&self) -> bool {
    self.executor.borrow().is_terminated()
  }

  /// Returns the identity of the shard boundary this system posts to.
  ///
  /// A waiting `ask` uses the same identity, taken from the recipient handle,
  /// to find the step that drives this system.
  pub fn shard_key(&self) -> usize {
    self.executor.borrow().boundary_key()
  }

  /// Returns the event sink the system publishes to, if one was installed.
  ///
  /// The test kit reads it back to arm an `EventFilter` for the duration of an
  /// `intercept`.
  pub fn event_sink(&self) -> Option<Arc<dyn crate::kernel::actor::Logging::EventSink>> {
    self.executor.borrow().event_sink()
  }
}

impl Drop for ActorSystem {
  fn drop(&mut self) {
    self.terminate();
  }
}

impl fmt::Debug for ActorSystem {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("ActorSystem")
      .field("name", &self.name.as_str())
      .field("terminated", &self.is_terminated())
      .finish()
  }
}
