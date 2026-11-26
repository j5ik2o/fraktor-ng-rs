// Ported from Pekko v2.0.0:
//   actor-tests/src/test/scala/org/apache/pekko/actor/ActorConfigurationVerificationSpec.scala
//
// Differences from the original, recorded as the design of issue #194
// (docs/design/issue-194-specs.md) decides:
// - The original defines its dispatchers in a HOCON string by class
//   name. The port hands typed values instead: `Dispatchers` in the
//   system setup, with the kind as an enum (precedents: #154 D1, G0b
//   decision E). The cases guard the meaning — a balancing dispatcher
//   cannot head a router, and an unknown dispatcher name is refused —
//   not the class-name loading itself.
// - The head-of-router message in Pekko's body carries the routee
//   dispatcher name by mistake. The port does not copy that mistake:
//   the head message names the head dispatcher. The shape of the
//   message (`not configured for router of`) is kept.
// - A dispatcher defined through `DispatchSettings` (the capacity
//   definitions) also counts as configured, because the existing ported
//   tests define their dispatchers that way; its kind is not balancing.
// - The router is minimal in this issue: verification and the head are
//   built, no routees are created, and a message to the head is
//   reported as unhandled. Routing itself is issue #269.

use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::{ActorCreationError, ConfigurationException, Props};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::dispatch::{DispatcherConfig, DispatcherKind, Dispatchers};
use fraktor_actor::kernel::actor::{PoolKind, RouterConfig};
use fraktor_actor::kernel::setup::ActorSystemSetup;

const SPEC_SYSTEM_NAME: &str = "ActorConfigurationVerificationSpec";

fn system_with_dispatchers() -> ActorSystem {
  let dispatchers = Dispatchers::new(vec![
    DispatcherConfig { id: "balancing-dispatcher".to_string(), kind: DispatcherKind::Balancing },
    DispatcherConfig { id: "pinned-dispatcher".to_string(), kind: DispatcherKind::Pinned },
  ]);
  let setup = ActorSystemSetup::empty().with_setup(dispatchers);
  ActorSystem::with_setup(SPEC_SYSTEM_NAME, Arc::new(ChannelBoundary::new()), setup)
}

fn router(pool: PoolKind, router_dispatcher: Option<&str>) -> RouterConfig {
  RouterConfig { pool, nr_of_instances: 5, router_dispatcher: router_dispatcher.map(str::to_string) }
}

fn expect_configuration_error(result: Result<fraktor_actor::kernel::actor_ref::ActorRef, ActorCreationError>) -> ConfigurationException {
  match result {
    | Err(ActorCreationError::Configuration(error)) => error,
    | Err(other) => panic!("expected a configuration error, got {other}"),
    | Ok(reference) => panic!("expected a configuration error, got actor {}", reference.path()),
  }
}

// Original: "fail verification with a ConfigurationException if also
// configured with a RoundRobinPool"
#[test]
fn fails_verification_when_a_balancing_dispatcher_is_configured_with_a_round_robin_pool() {
  let mut system = system_with_dispatchers();
  let props = Props::empty().with_router(router(PoolKind::RoundRobin, Some("balancing-dispatcher")));
  let _ = expect_configuration_error(system.try_actor_of(props, None));
  system.terminate();
}

// Original: "fail verification with a ConfigurationException if also
// configured with a BroadcastPool"
#[test]
fn fails_verification_when_a_balancing_dispatcher_is_configured_with_a_broadcast_pool() {
  let mut system = system_with_dispatchers();
  let props = Props::empty().with_router(router(PoolKind::Broadcast, Some("balancing-dispatcher")));
  let _ = expect_configuration_error(system.try_actor_of(props, None));
  system.terminate();
}

// Original: "fail verification with a ConfigurationException if also
// configured with a RandomPool"
#[test]
fn fails_verification_when_a_balancing_dispatcher_is_configured_with_a_random_pool() {
  let mut system = system_with_dispatchers();
  let props = Props::empty().with_router(router(PoolKind::Random, Some("balancing-dispatcher")));
  let _ = expect_configuration_error(system.try_actor_of(props, None));
  system.terminate();
}

// Original: "fail verification with a ConfigurationException if also
// configured with a SmallestMailboxPool"
#[test]
fn fails_verification_when_a_balancing_dispatcher_is_configured_with_a_smallest_mailbox_pool() {
  let mut system = system_with_dispatchers();
  let props = Props::empty().with_router(router(PoolKind::SmallestMailbox, Some("balancing-dispatcher")));
  let _ = expect_configuration_error(system.try_actor_of(props, None));
  system.terminate();
}

// Original: "fail verification with a ConfigurationException if also
// configured with a ScatterGatherFirstCompletedPool"
#[test]
fn fails_verification_when_a_balancing_dispatcher_is_configured_with_a_scatter_gather_first_completed_pool() {
  let mut system = system_with_dispatchers();
  let pool = PoolKind::ScatterGatherFirstCompleted { within: core::time::Duration::from_secs(2) };
  let props = Props::empty().with_router(router(pool, Some("balancing-dispatcher")));
  let _ = expect_configuration_error(system.try_actor_of(props, None));
  system.terminate();
}

// Original: "not fail verification with a ConfigurationException also
// not configured with a Router"
#[test]
fn does_not_fail_verification_when_a_balancing_dispatcher_is_used_without_a_router() {
  let mut system = system_with_dispatchers();
  let props = Props::empty().with_dispatcher("balancing-dispatcher");
  let created = system.try_actor_of(props, None).expect("a balancing dispatcher without a router is accepted");
  assert!(!system.is_actor_terminated(&created));
  system.terminate();
}

// Original: "not fail verification with a ConfigurationException if
// also configured with a Router"
#[test]
fn does_not_fail_verification_when_a_non_balancing_dispatcher_is_configured_with_a_router() {
  let mut system = system_with_dispatchers();
  // The routees use the pinned dispatcher; the head keeps the default.
  let props = Props::empty().with_dispatcher("pinned-dispatcher").with_router(router(PoolKind::RoundRobin, None));
  let created = system.try_actor_of(props, None).expect("a router whose routees use a pinned dispatcher is accepted");
  assert!(!system.is_actor_terminated(&created));
  system.terminate();
}

// Original: "fail verification if the dispatcher cannot be found"
#[test]
fn fails_verification_if_the_dispatcher_cannot_be_found() {
  let mut system = system_with_dispatchers();
  let props = Props::empty().with_dispatcher("does not exist");
  let error = expect_configuration_error(system.try_actor_of(props, None));
  assert!(error.to_string().contains("Dispatcher [does not exist] not configured"), "unexpected message: {error}");
  system.terminate();
}

// Original: "fail verification if the dispatcher cannot be found for
// the head of a router"
#[test]
fn fails_verification_if_the_dispatcher_cannot_be_found_for_the_head_of_a_router() {
  let mut system = system_with_dispatchers();
  let props = Props::empty().with_router(router(PoolKind::RoundRobin, Some("does not exist")));
  let error = expect_configuration_error(system.try_actor_of(props, None));
  assert!(error.to_string().contains("Dispatcher [does not exist] not configured for router of"), "unexpected message: {error}");
  system.terminate();
}

// Original: "fail verification if the dispatcher cannot be found for
// the routees of a router"
#[test]
fn fails_verification_if_the_dispatcher_cannot_be_found_for_the_routees_of_a_router() {
  let mut system = system_with_dispatchers();
  let props = Props::empty().with_dispatcher("does not exist").with_router(router(PoolKind::RoundRobin, None));
  let error = expect_configuration_error(system.try_actor_of(props, None));
  assert!(error.to_string().contains("Dispatcher [does not exist] not configured for routees of"), "unexpected message: {error}");
  system.terminate();
}
