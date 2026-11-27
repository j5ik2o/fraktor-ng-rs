// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A test base that owns an actor system.

use core::time::Duration;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::adaptor::UnwindCatcher;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

use crate::event_filter::EventFilter;
use crate::event_filter::EventSinkRegistry;
use crate::fraktorspec::fraktor_spec_hooks::FraktorSpecHooks;
use crate::test_kit::expected_messages::ClassOf;
use crate::test_kit::AutoPilot;
use crate::test_kit::ReceiveWhile;
use crate::test_kit::TestKit;
use crate::test_kit_extension::TestKitSettings;
use crate::test_kit_extension::TestTimeFactor;

#[cfg(test)]
#[path = "fraktor_spec_test.rs"]
mod tests;

/// Owns the actor system of a test and stops it when the test is done.
///
/// Mirrors `org.apache.pekko.testkit.PekkoSpec` for the parts that the ported
/// tests need. Pekko extends `TestKit`, which owns the system and the test
/// actor, and mixes in `BeforeAndAfterAll`, whose `beforeAll` runs `atStartup`
/// and whose `afterAll` runs `beforeTermination`, `shutdown`, and
/// `afterTermination`. The port composes a `TestKit` instead of extending it,
/// takes the hooks as an object, and makes `terminate` play the role of
/// `afterAll`. The parts of the original that belong to later goals are not
/// ported: the coroner, `muteDeadLetters`, `spawn`, `log`, the configuration
/// constructors, `patience`, and the equality constraints.
///
/// The system is created with a panic catcher, so a failing actor of the test
/// becomes a supervisor decision instead of unwinding through the test. Pekko
/// gets the same behaviour from its dispatcher.
pub struct FraktorSpec {
  test_kit: TestKit,
  hooks: Box<dyn FraktorSpecHooks>,
  terminated: bool,
}

impl FraktorSpec {
  /// Creates the actor system with the given name, its test kit, and the test
  /// actor, then runs `at_startup`.
  pub fn with_hooks(system_name: &str, hooks: Box<dyn FraktorSpecHooks>) -> Self {
    Self::with_hooks_and_settings(system_name, hooks, None)
  }

  /// Creates the spec with `factor` as the test time factor and no hooks.
  ///
  /// Mirrors `PekkoSpec(Map("pekko.test.timefactor" -> factor))`: every dilated
  /// duration the spec and the probes built from it use is scaled by `factor`.
  pub fn with_time_factor(system_name: &str, factor: TestTimeFactor) -> Self {
    Self::with_hooks_and_settings(system_name, Box::new(NoHooks), Some(factor))
  }

  fn with_hooks_and_settings(
    system_name: &str,
    hooks: Box<dyn FraktorSpecHooks>,
    factor: Option<TestTimeFactor>,
  ) -> Self {
    let system =
      ActorSystem::with_failure_catcher(system_name, Arc::new(ChannelBoundary::new()), Arc::new(UnwindCatcher));
    let registry = Arc::new(EventSinkRegistry::new());
    system.set_event_sink(registry);
    fraktor_actor::adaptor::attach_wait_driver(&system);
    let settings = match factor {
      | Some(factor) => TestKitSettings::with_time_factor(factor),
      | None => TestKitSettings::from_system(&system),
    };
    let mut spec = Self { test_kit: TestKit::with_settings(system, settings), hooks, terminated: false };
    spec.hooks.at_startup(&mut spec.test_kit);
    spec
  }

  /// Creates the spec with the given name and no startup or shutdown hooks.
  pub fn new(system_name: &str) -> Self {
    Self::with_hooks(system_name, Box::new(NoHooks))
  }

  /// Returns the actor system owned by this spec.
  pub fn system(&self) -> &ActorSystem {
    self.test_kit.system()
  }

  /// Returns the actor system owned by this spec, mutably.
  pub fn system_mut(&mut self) -> &mut ActorSystem {
    self.test_kit.system_mut()
  }

  /// Returns the test kit of this spec, corresponding to the `TestKitBase` that
  /// the spec inherits.
  pub fn test_kit_mut(&mut self) -> &mut TestKit {
    &mut self.test_kit
  }

  /// Returns the test kit settings of the actor system.
  pub fn test_kit_settings(&self) -> TestKitSettings {
    self.test_kit.test_kit_settings()
  }

  /// Returns the handle of the test actor, corresponding to Pekko's `testActor`.
  pub fn test_actor(&self) -> ActorRef {
    self.test_kit.test_actor()
  }

  /// Waits for the next message of the test actor and checks it, corresponding
  /// to Pekko's `expectMsg`.
  pub fn expect_msg<E>(&mut self, expected: &E) -> Message
  where
    E: ?Sized + core::fmt::Debug,
    Message: PartialEq<E>,
  {
    self.test_kit.expect_msg(expected)
  }

  /// Waits at most `max` for the next message and checks it.
  pub fn expect_msg_within<E>(&mut self, max: Duration, expected: &E) -> Message
  where
    E: ?Sized + core::fmt::Debug,
    Message: PartialEq<E>,
  {
    self.test_kit.expect_msg_within(max, expected)
  }

  /// Waits for the next message and returns it when it carries a `T`.
  pub fn expect_msg_type<T: core::any::Any + Clone>(&mut self) -> T {
    self.test_kit.expect_msg_type()
  }

  /// Waits for the next message and returns it when it is of one of `classes`.
  pub fn expect_msg_any_class_of<T: core::any::Any + Clone>(&mut self, classes: &[ClassOf<T>]) -> T {
    self.test_kit.expect_msg_any_class_of(classes)
  }

  /// Waits for one message per entry in `classes` and returns them.
  pub fn expect_msg_all_class_of<T: core::any::Any + Clone>(&mut self, classes: &[ClassOf<T>]) -> Vec<T> {
    self.test_kit.expect_msg_all_class_of(classes)
  }

  /// Waits at most `max` for one message per entry in `classes`.
  pub fn expect_msg_all_class_of_within<T: core::any::Any + Clone>(
    &mut self,
    max: Duration,
    classes: &[ClassOf<T>],
  ) -> Vec<T> {
    self.test_kit.expect_msg_all_class_of_within(max, classes)
  }

  /// Waits for one message per entry in `classes` and returns them.
  pub fn expect_msg_all_conforming_of<T: core::any::Any + Clone>(&mut self, classes: &[ClassOf<T>]) -> Vec<T> {
    self.test_kit.expect_msg_all_conforming_of(classes)
  }

  /// Waits at most `max` for one message per entry in `classes`.
  pub fn expect_msg_all_conforming_of_within<T: core::any::Any + Clone>(
    &mut self,
    max: Duration,
    classes: &[ClassOf<T>],
  ) -> Vec<T> {
    self.test_kit.expect_msg_all_conforming_of_within(max, classes)
  }

  /// Installs the pilot that runs for each message of the test actor before it
  /// is queued, corresponding to `setAutoPilot`.
  pub fn set_auto_pilot<P: AutoPilot + 'static>(&self, pilot: P) {
    self.test_kit.set_auto_pilot(pilot);
  }

  /// Takes messages of the test actor while `pick` accepts them, corresponding
  /// to `receiveWhile`.
  pub fn receive_while<T>(&mut self, limits: ReceiveWhile, pick: impl FnMut(&Message) -> Option<T>) -> Vec<T> {
    self.test_kit.receive_while(limits, pick)
  }

  /// Waits until `condition` holds or the dilated `max` has passed,
  /// corresponding to `awaitCond`.
  ///
  /// The system keeps running between evaluations, also after `terminate`,
  /// where a condition such as "every actor is terminated" reads the system
  /// through a [`WeakActorSystem`](fraktor_actor::kernel::actor_system::WeakActorSystem)
  /// taken before the call.
  pub fn await_cond(&mut self, condition: impl FnMut() -> bool, max: Duration) {
    self.test_kit.await_cond(condition, max);
  }

  /// Runs `assertion` until it stops failing, corresponding to `awaitAssert`.
  pub fn await_assert<T>(&mut self, max: Duration, interval: Duration, assertion: impl FnMut() -> T) -> T {
    self.test_kit.await_assert(max, interval, assertion)
  }

  /// Runs `assertion` again and again until it stops failing or `max` passes,
  /// corresponding to ScalaTest's `Eventually`.
  pub fn eventually(&mut self, max: Duration, interval: Duration, assertion: impl FnMut()) {
    self.test_kit.eventually(max, interval, assertion)
  }

  /// Runs `body` with its waits clamped to the `max` that is left.
  ///
  /// Mirrors `TestKitBase.within`: `expectMsg`, `expectNoMessage`, and the
  /// waits those share use the remaining time of the scope when that is shorter
  /// than the wait they ask for, so a final `expectNoMessage` checks up to the
  /// end of the scope and no further. `awaitAssert` and `eventually` take the
  /// `max` they are given and do not use the scope's remaining time. A nested
  /// `within` keeps the earlier deadline, and the deadline is restored even
  /// when `body` fails. Once `body` returns, the call fails when it took longer
  /// than the scope, unless the last wait was a no-message check that used up
  /// the scope on purpose; the no-message flag is cleared at the entry of this
  /// call and by `receiveOne`, which are the only two places that set it back
  /// to `false`.
  pub fn within<T>(&mut self, max: Duration, body: impl FnOnce(&mut Self) -> T) -> T {
    self.within_range(Duration::ZERO, max, body)
  }

  /// Runs `body` with its waits clamped to the `max` that is left, and checks
  /// that it ran for at least `min` and at most the scope.
  ///
  /// Mirrors `TestKitBase.within(min, max)`: `max` is dilated but `min` is not.
  /// When the surrounding scope has less than `min` left, the call fails before
  /// running `body`; otherwise the waits clamp to the scope's deadline. Once
  /// `body` returns, the call fails when it ran for less than `min`, and -- when
  /// the last wait was not a no-message check -- for longer than the scope. The
  /// deadline is restored even when `body` fails.
  ///
  /// # Panics
  ///
  /// Panics when the surrounding scope cannot satisfy `min`, when `body` ran for
  /// less than `min`, or when it exceeded the scope.
  pub fn within_range<T>(&mut self, min: Duration, max: Duration, body: impl FnOnce(&mut Self) -> T) -> T {
    let scope = self.test_kit.scope();
    let previous = *scope.borrow();
    let started = Instant::now();
    let dilated_max = self.test_kit.test_kit_settings().dilated(max);
    if let Some(previous) = previous {
      let remaining = previous.saturating_duration_since(started);
      assert!(remaining >= min, "required min time {min:?} not possible, only {remaining:?} left");
    }
    let requested = started + dilated_max;
    let deadline = previous.map_or(requested, |previous| requested.min(previous));
    let max_diff = deadline.saturating_duration_since(started);
    *scope.borrow_mut() = Some(deadline);
    self.test_kit.reset_last_was_no_msg();
    let _reset = ScopeReset { scope, previous };
    let value = body(self);
    let elapsed = started.elapsed();
    assert!(min <= elapsed, "block took {elapsed:?}, should at least have been {min:?}");
    if !self.test_kit.last_was_no_msg() {
      assert!(elapsed <= max_diff, "block took {elapsed:?}, exceeding {max_diff:?}");
    }
    value
  }

  /// Runs `body` while the spec's sink counts the events `filter`
  /// matches, then checks that the number published is the filter's
  /// occurrences.
  ///
  /// Counting is all the sink does with a match: the executor offers
  /// an event to the event stream and to the sink separately, so a
  /// matching event still reaches the stream's subscribers. The
  /// intercept verifies the count; it does not suppress delivery.
  ///
  /// The `FraktorSpec` form of [`EventFilter::intercept`]: the body
  /// receives this spec, so it can call `within`, `expect_msg`, and
  /// `actor_of` while the filter is armed, which the
  /// `&mut ActorSystem` form cannot offer without a borrow conflict.
  ///
  /// # Panics
  ///
  /// Panics when the spec's system has no event sink, or when the
  /// number of matching events after the body differs from the
  /// filter's occurrences.
  pub fn intercept<R>(&mut self, filter: &EventFilter, body: impl FnOnce(&mut Self) -> R) -> R {
    let registry = self.event_registry();
    let id = registry.arm(filter.clone());
    let _guard = ArmedGuard { registry: registry.clone(), ids: vec![id] };
    let outcome = body(self);
    self.system_mut().run_until_idle();
    let matched = registry.matched(id);
    assert_eq!(
      matched,
      filter.occurrences(),
      "expected {} event(s) matching {filter:?} but found {matched}",
      filter.occurrences()
    );
    outcome
  }

  /// Runs `body` while the spec's sink counts the events every
  /// filter in `filters` matches, then checks that each filter matched
  /// its occurrences. As with [`intercept`](Self::intercept), a match
  /// is only counted; the event still reaches the event stream's
  /// subscribers.
  ///
  /// Mirrors Pekko's `filterEvents(filters)(block)`: every filter is
  /// armed at once, the body runs on this spec, the system is driven
  /// to idle, and each filter's count is compared with its own
  /// occurrences before all of them are disarmed.
  ///
  /// # Panics
  ///
  /// Panics when the spec's system has no event sink, or when any
  /// filter's count after the body differs from its occurrences.
  pub fn filter_events<R>(&mut self, filters: &[EventFilter], body: impl FnOnce(&mut Self) -> R) -> R {
    let registry = self.event_registry();
    let ids: Vec<usize> = filters.iter().map(|filter| registry.arm(filter.clone())).collect();
    let _guard = ArmedGuard { registry: registry.clone(), ids: ids.clone() };
    let outcome = body(self);
    self.system_mut().run_until_idle();
    let matched: Vec<usize> = registry.matched_all(&ids);
    for (filter, count) in filters.iter().zip(matched.iter()) {
      assert_eq!(
        *count,
        filter.occurrences(),
        "expected {} event(s) matching {filter:?} but found {count}",
        filter.occurrences()
      );
    }
    outcome
  }

  /// Returns the event sink registry this spec's system publishes to.
  fn event_registry(&self) -> Arc<EventSinkRegistry> {
    let sink = self.system().event_sink().expect("the spec system has an event sink");
    sink.into_any().downcast::<EventSinkRegistry>().expect("the spec uses an EventSinkRegistry")
  }

  /// Returns the time left in the innermost `within` scope.
  ///
  /// Mirrors `TestKitBase.remaining`: the time left saturates to zero once the
  /// deadline has passed.
  ///
  /// # Panics
  ///
  /// Panics when called outside of a `within` block.
  pub fn remaining(&self) -> Duration {
    self.test_kit.remaining()
  }

  /// Stops the actor system, corresponding to Pekko's `afterAll`.
  ///
  /// Runs `before_termination`, then the shutdown of the test kit, then
  /// `after_termination`, in that order. A second call is a no-op, as `afterAll`
  /// runs once.
  pub fn terminate(&mut self) {
    if self.terminated {
      return;
    }
    self.hooks.before_termination(&mut self.test_kit);
    self.test_kit.shutdown();
    self.hooks.after_termination(&mut self.test_kit);
    self.terminated = true;
  }
}

struct NoHooks;

impl FraktorSpecHooks for NoHooks {}

/// Restores the surrounding `within` deadline when a scope ends.
struct ScopeReset {
  scope: Rc<RefCell<Option<Instant>>>,
  previous: Option<Instant>,
}

impl Drop for ScopeReset {
  fn drop(&mut self) {
    *self.scope.borrow_mut() = self.previous;
  }
}

/// Disarms the filters an `intercept` or a `filter_events` armed when
/// the scope ends, so a body that panics leaves no filter behind on
/// the registry to match the events of a later intercept.
struct ArmedGuard {
  registry: Arc<EventSinkRegistry>,
  ids: Vec<usize>,
}

impl Drop for ArmedGuard {
  fn drop(&mut self) {
    for id in &self.ids {
      self.registry.disarm(*id);
    }
  }
}
