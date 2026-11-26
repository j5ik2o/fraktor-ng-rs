//! The table of dispatcher definitions a system verifies against.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::kernel::setup::Setup;

/// The id of the dispatcher Pekko uses when none is named.
///
/// Mirrors `Dispatchers.DefaultDispatcherId`.
pub const DEFAULT_DISPATCHER_ID: &str = "pekko.actor.default-dispatcher";

/// The kind of a dispatcher, as the design of #234 (section 3.5)
/// defines it.
///
/// The verification rules of this issue only ask whether a kind is
/// `Balancing`; the execution of a dispatcher belongs to #234.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DispatcherKind {
  /// The default dispatcher.
  Default,
  /// A fork-join pool dispatcher.
  ForkJoin,
  /// A dispatcher that pins its actors to one thread.
  Pinned,
  /// A dispatcher that shares one mailbox between its actors.
  Balancing,
  /// A dispatcher that runs on the calling thread.
  CallingThread,
  /// A dispatcher the test kit drives by hand.
  Manual,
}

/// One dispatcher definition, built in code (no config file).
///
/// Mirrors one entry of the `pekko.actor.dispatchers` configuration in
/// `org.apache.pekko.dispatch.Dispatchers`, with the configurator class
/// name replaced by the [`DispatcherKind`] enum.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DispatcherConfig {
  /// The id actors name through `Props::with_dispatcher`.
  pub id: String,
  /// The kind of dispatcher the id stands for.
  pub kind: DispatcherKind,
}

/// The table of dispatcher ids a system knows, with the default id.
///
/// Mirrors `org.apache.pekko.dispatch.Dispatchers`: the lookup side
/// only. Registering a definition makes its id known; the default id
/// is always known. The table travels into a system through
/// [`ActorSystemSetup`](crate::kernel::setup::ActorSystemSetup), as a
/// `Setup`, and stage 5 of #234 extends this same type with aliases
/// and execution instead of replacing it (design of #194, M1).
///
/// A dispatcher that a [`DispatchSettings`](crate::kernel::actor::dispatch_settings::DispatchSettings)
/// defines also counts as configured for verification, because the
/// ported tests define their dispatchers that way; its kind reads as
/// not balancing, as only this table records kinds.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Dispatchers {
  configs: Vec<DispatcherConfig>,
  default_id: String,
}

impl Dispatchers {
  /// Creates a table holding `configs` plus the default dispatcher.
  pub fn new(configs: Vec<DispatcherConfig>) -> Self {
    Self { configs, default_id: DEFAULT_DISPATCHER_ID.to_string() }
  }

  /// Returns the table with only the default dispatcher.
  pub fn with_defaults() -> Self {
    Self::new(Vec::new())
  }

  /// Returns whether `id` names a dispatcher in this table.
  ///
  /// Mirrors `Dispatchers.hasDispatcher`.
  pub fn has_dispatcher(&self, id: &str) -> bool {
    id == self.default_id || self.configs.iter().any(|config| config.id == id)
  }

  /// Returns the kind of the dispatcher `id` names, when known.
  pub fn kind_of(&self, id: &str) -> Option<DispatcherKind> {
    if id == self.default_id {
      return Some(DispatcherKind::Default);
    }
    self.configs.iter().find(|config| config.id == id).map(|config| config.kind)
  }

  /// Returns the id of the default dispatcher.
  pub fn default_id(&self) -> &str {
    &self.default_id
  }
}

impl Default for Dispatchers {
  fn default() -> Self {
    Self::with_defaults()
  }
}

impl Setup for Dispatchers {}
