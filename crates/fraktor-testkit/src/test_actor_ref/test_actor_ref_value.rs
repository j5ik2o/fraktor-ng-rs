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

//! The handle a test holds on an actor it started.

use core::any::Any;
use core::time::Duration;

use std::time::Instant;

use fraktor_actor::kernel::actor::test_support::ActorHandle;
use fraktor_actor::kernel::actor::{Actor, ActorContext, ActorCreationError, Props, ReceiveTimeoutSetting};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::{ActorSystem, WeakActorSystem};
use fraktor_actor::kernel::dispatch::DispatcherKind;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::RunOutcome;

use crate::test_actor_ref::CALLING_THREAD_DISPATCHER_ID;

/// How long a read of the underlying actor waits for its probe, the
/// test kit's default timeout, which is what Pekko's `underlyingActor`
/// waits with (`DefaultTimeout`).
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// The two shapes a creation reaches this handle through: the system a
/// test owns, which the test thread drives while it waits for the
/// actor to start, and the non-owning handle an actor builds a nested
/// reference from, whose creation cannot drive the system because the
/// actor is running inside it. The ported cases call the openings with
/// both, so the openings accept both through this one face.
trait StartSystem {
  /// Returns the non-owning handle the reference keeps.
  fn weak_system(&self) -> WeakActorSystem;
  /// Returns the dead letters handle of the system.
  fn dead_letters_ref(&self) -> ActorRef;
  /// Returns whether the creation waits for the actor to start.
  fn waits_for_start(&self) -> bool;
}

impl StartSystem for &mut ActorSystem {
  fn weak_system(&self) -> WeakActorSystem {
    self.downgrade()
  }

  fn dead_letters_ref(&self) -> ActorRef {
    self.dead_letters()
  }

  fn waits_for_start(&self) -> bool {
    true
  }
}

impl StartSystem for WeakActorSystem {
  fn weak_system(&self) -> WeakActorSystem {
    self.clone()
  }

  fn dead_letters_ref(&self) -> ActorRef {
    self.dead_letters()
  }

  fn waits_for_start(&self) -> bool {
    false
  }
}

/// A handle on a running actor that a test reads and drives directly.
///
/// Mirrors `org.apache.pekko.testkit.TestActorRef`: the actor starts on
/// the calling-thread dispatcher when its recipe names none, a test
/// reads a copy of the actor's state instead of the actor itself (the
/// port does not hand the actor out), and `receive` runs the behaviour
/// once outside the mailbox.
///
/// A reference built in a system that does not know the
/// calling-thread dispatcher -- one a plain `ActorSystem::new` built,
/// without the testkit's setup -- is refused with
/// [`ActorCreationError::Configuration`], as Pekko refuses it when the
/// testkit's `reference.conf` is absent.
pub struct TestActorRef {
  actor_ref: ActorRef,
  handle: ActorHandle,
  name: String,
  supervisor: ActorRef,
  system: WeakActorSystem,
  dead_letters: ActorRef,
}

#[allow(private_bounds)]
impl TestActorRef {
  /// Starts the actor `factory` builds below the guardian, under a
  /// generated name.
  ///
  /// Mirrors `TestActorRef(factory)`.
  ///
  /// # Panics
  ///
  /// Panics when the actor fails to start, as Pekko's creation throws
  /// the initialization failure instead of handing out a reference.
  pub fn new<F, A>(system: &mut ActorSystem, factory: F) -> Self
  where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor + 'static,
  {
    Self::from_props(system, Props::new(factory))
  }

  /// Starts the actor `factory` builds below the guardian, under
  /// `name`.
  ///
  /// Mirrors `TestActorRef(factory, name)`.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name is invalid or in use.
  ///
  /// # Panics
  ///
  /// Panics when the actor fails to start.
  pub fn with_name<F, A>(system: &mut ActorSystem, factory: F, name: &str) -> Result<Self, ActorCreationError>
  where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor + 'static,
  {
    Self::from_props_with_name(system, Props::new(factory), name)
  }

  /// Starts the actor `factory` builds as a child of `supervisor`,
  /// under a generated name.
  ///
  /// Mirrors `TestActorRef(factory, supervisor)`.
  ///
  /// # Panics
  ///
  /// Panics when the actor fails to start.
  pub fn with_supervisor<F, A>(system: &mut ActorSystem, factory: F, supervisor: &ActorRef) -> Self
  where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor + 'static,
  {
    Self::from_props_with_supervisor(system, Props::new(factory), supervisor)
  }

  /// Starts the actor `factory` builds as a child of `supervisor`,
  /// under `name`.
  ///
  /// Mirrors `TestActorRef(factory, supervisor, name)`.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name is invalid or in use.
  ///
  /// # Panics
  ///
  /// Panics when the actor fails to start.
  pub fn with_supervisor_and_name<F, A>(
    system: &mut ActorSystem,
    factory: F,
    supervisor: &ActorRef,
    name: &str,
  ) -> Result<Self, ActorCreationError>
  where
    F: Fn() -> A + Send + Sync + 'static,
    A: Actor + 'static,
  {
    Self::from_props_with_supervisor_and_name(system, Props::new(factory), supervisor, name)
  }

  /// Starts the actor of `props` below the guardian, under a generated
  /// name. The system may be the one a test owns or the non-owning
  /// handle an actor runs in, which is how an actor builds a nested
  /// reference for a worker it starts.
  ///
  /// Mirrors `TestActorRef(props)`.
  ///
  /// # Panics
  ///
  /// Panics when the actor fails to start, or when the creation is
  /// refused, as Pekko's creation throws instead of handing out a
  /// reference.
  pub fn from_props<S>(system: S, props: Props) -> Self
  where
    S: StartSystem,
  {
    match Self::start(&system, props, None, None) {
      | Ok(started) => started,
      | Err(error) => panic!("{error}"),
    }
  }

  /// Starts the actor of `props` below the guardian, under `name`.
  ///
  /// Mirrors `TestActorRef(props, name)`.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name is invalid or in use.
  ///
  /// # Panics
  ///
  /// Panics when the actor fails to start.
  pub fn from_props_with_name<S>(system: S, props: Props, name: &str) -> Result<Self, ActorCreationError>
  where
    S: StartSystem,
  {
    Self::start(&system, props, None, Some(name))
  }

  /// Starts the actor of `props` as a child of `supervisor`, under a
  /// generated name.
  ///
  /// Mirrors `TestActorRef(props, supervisor)`.
  ///
  /// # Panics
  ///
  /// Panics when the actor fails to start, or when the creation is
  /// refused.
  pub fn from_props_with_supervisor<S>(system: S, props: Props, supervisor: &ActorRef) -> Self
  where
    S: StartSystem,
  {
    match Self::start(&system, props, Some(supervisor), None) {
      | Ok(started) => started,
      | Err(error) => panic!("{error}"),
    }
  }

  /// Starts the actor of `props` as a child of `supervisor`, under
  /// `name`.
  ///
  /// Mirrors `TestActorRef(props, supervisor, name)`.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name is invalid or in use.
  ///
  /// # Panics
  ///
  /// Panics when the actor fails to start.
  pub fn from_props_with_supervisor_and_name<S>(
    system: S,
    props: Props,
    supervisor: &ActorRef,
    name: &str,
  ) -> Result<Self, ActorCreationError>
  where
    S: StartSystem,
  {
    Self::start(&system, props, Some(supervisor), Some(name))
  }

  /// Starts the actor of `props` from inside an actor, under the
  /// supervisor and name given, or below the guardian and under a
  /// generated name when they are omitted.
  ///
  /// Mirrors the same `TestActorRef` openings called with the implicit
  /// `context.system` of a Pekko actor's constructor; the port builds
  /// the nested actor from `pre_start`, where the context exists. The
  /// creation does not wait for the actor to start, because the system
  /// cannot be driven from inside an actor that is running in it.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name, the parent's state,
  /// or the configuration is refused.
  pub fn in_context(
    context: &mut ActorContext<'_>,
    props: Props,
    supervisor: Option<&ActorRef>,
    name: Option<&str>,
  ) -> Result<Self, ActorCreationError> {
    let system = context.system();
    let dead_letters = context.dead_letters();
    let props = Self::with_calling_thread(props);
    let handle = ActorHandle::new();
    let actor_ref = context.try_spawn_test_actor(supervisor, props, name, handle.clone())?;
    let supervisor = match supervisor {
      | Some(supervisor) => supervisor.clone(),
      | None => system.resolve_actor_ref(&actor_ref.path().parent()),
    };
    let name = actor_ref.path().name().to_string();
    Ok(Self { actor_ref, handle, name, supervisor, system, dead_letters })
  }

  fn start<S>(
    system: &S,
    props: Props,
    supervisor: Option<&ActorRef>,
    name: Option<&str>,
  ) -> Result<Self, ActorCreationError>
  where
    S: StartSystem,
  {
    let weak = system.weak_system();
    let dead_letters = system.dead_letters_ref();
    let props = Self::with_calling_thread(props);
    let handle = ActorHandle::new();
    let actor_ref = weak.try_spawn_test_actor(supervisor, props, name, handle.clone())?;
    let supervisor = match supervisor {
      | Some(supervisor) => supervisor.clone(),
      | None => weak.resolve_actor_ref(&actor_ref.path().parent()),
    };
    let name = actor_ref.path().name().to_string();
    let started = Self { actor_ref, handle, name, supervisor, system: weak, dead_letters };
    if system.waits_for_start() {
      started.wait_for_start();
    }
    Ok(started)
  }

  /// Replaces the recipe's dispatcher with the calling-thread one
  /// when the recipe names none, as Pekko's `TestActorRef` replaces a
  /// `Deploy` with no dispatcher given. A recipe that names one, even
  /// the default dispatcher's own name, keeps it.
  fn with_calling_thread(props: Props) -> Props {
    match props.dispatcher() {
      | Some(_) => props,
      | None => props.with_dispatcher(CALLING_THREAD_DISPATCHER_ID),
    }
  }

  /// Drives the system until the actor has started or has stopped, as
  /// Pekko's calling-thread creation throws a start failure at the
  /// creation instead of handing out a reference.
  ///
  /// # Panics
  ///
  /// Panics when the actor stopped before it started.
  fn wait_for_start(&self) {
    while !self.handle.has_started() && !self.system.is_actor_terminated(&self.actor_ref) {
      if !self.system.run_one() {
        break;
      }
    }
    if self.system.is_actor_terminated(&self.actor_ref) && !self.handle.has_started() {
      panic!("the actor at {} failed to start", self.actor_ref.path());
    }
  }

  /// Returns the handle of the actor this reference wraps.
  pub fn actor_ref(&self) -> ActorRef {
    self.actor_ref.clone()
  }

  /// Sends `message` to the actor, naming `sender` as the sender.
  ///
  /// Mirrors `TestActorRef.!`.
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef) {
    self.actor_ref.tell(message, sender);
  }

  /// Returns the name the actor was started under.
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Returns the parent the actor was started under.
  pub fn supervisor(&self) -> ActorRef {
    self.supervisor.clone()
  }

  /// Returns whether the actor has stopped.
  ///
  /// Mirrors `TestActorRef.isTerminated`.
  pub fn is_terminated(&self) -> bool {
    self.system.is_actor_terminated(&self.actor_ref)
  }

  /// Runs the actor's behaviour once with `message`, outside its
  /// mailbox, with the dead letters handle as the sender.
  ///
  /// Mirrors `TestActorRef.receive(o)`.
  ///
  /// # Panics
  ///
  /// Panics with the failure's message when the behaviour fails, as
  /// the exception Pekko's `receive` lets through reaches its caller,
  /// and when the actor has stopped.
  pub fn receive<M: Any + Send + Sync>(&self, message: M) {
    let sender = self.dead_letters.clone();
    self.receive_with(message, sender);
  }

  /// Runs the actor's behaviour once with `message`, outside its
  /// mailbox, naming `sender` as the sender.
  ///
  /// Mirrors `TestActorRef.receive(o, sender)`.
  ///
  /// # Panics
  ///
  /// Panics as [`receive`](Self::receive) does.
  pub fn receive_with_sender<M: Any + Send + Sync>(&self, message: M, sender: ActorRef) {
    self.receive_with(message, sender);
  }

  fn receive_with<M: Any + Send + Sync>(&self, message: M, sender: ActorRef) {
    let outcome = self.system.receive_directly(&self.actor_ref, Message::new(message), sender);
    if let RunOutcome::Failed { message } = outcome {
      panic!("{message}");
    }
  }

  /// Returns a copy of the actor's state of the moment a probe
  /// processed it, of the type the test names.
  ///
  /// Mirrors reading a field off Pekko's `underlyingActor`; the port
  /// does not hand out the actor, so the actor publishes the copy
  /// through `Actor::underlying_snapshot` and the probe takes it in
  /// mailbox order.
  ///
  /// # Panics
  ///
  /// Panics when the actor has stopped, with Pekko's
  /// `IllegalActorStateException` cause, when the actor publishes no
  /// snapshot or one of another type, and when the probe is not
  /// processed within the default timeout.
  pub fn underlying_snapshot<T: Any + Clone + Send + Sync>(&self) -> T {
    self.sync_with_probe();
    match self.handle.read_snapshot::<T>() {
      | Some(snapshot) => snapshot,
      | None => {
        if self.handle.has_snapshot() {
          panic!("the actor at {} published an underlying snapshot of another type", self.actor_ref.path());
        }
        panic!("the actor at {} does not provide an underlying snapshot", self.actor_ref.path());
      },
    }
  }

  /// Returns the receive timeout the actor had when the last probe
  /// was processed.
  ///
  /// Mirrors `underlyingActor.context.receiveTimeout`.
  ///
  /// # Panics
  ///
  /// Panics as [`underlying_snapshot`](Self::underlying_snapshot)
  /// does, apart from the snapshot's own causes.
  pub fn receive_timeout(&self) -> ReceiveTimeoutSetting {
    self.sync_with_probe();
    self.handle.receive_timeout()
  }

  /// Returns the kind of the dispatcher the executor resolved for
  /// the actor at creation.
  ///
  /// Mirrors reading the runtime class of Pekko's
  /// `underlying.dispatcher`; the port has no runtime class, so it
  /// answers with the resolved kind instead.
  pub fn dispatcher_kind(&self) -> Option<DispatcherKind> {
    self.handle.dispatcher_kind()
  }

  /// Registers the wrapped actor as a watcher of `subject` and
  /// returns `subject`.
  ///
  /// Mirrors `TestActorRef.watch`.
  pub fn watch(&self, subject: &ActorRef) -> ActorRef {
    self.system.watch(&self.actor_ref, subject);
    subject.clone()
  }

  /// Stops the wrapped actor from watching `subject` and returns
  /// `subject`.
  ///
  /// Mirrors `TestActorRef.unwatch`.
  pub fn unwatch(&self, subject: &ActorRef) -> ActorRef {
    self.system.unwatch(&self.actor_ref, subject);
    subject.clone()
  }

  /// Sends a probe to the actor and drives the system until the actor
  /// has processed it or has stopped.
  ///
  /// # Panics
  ///
  /// Panics when the actor has stopped, before the probe is sent, as
  /// it is sent, or while it is waited for, and when the probe is not
  /// processed within the default timeout.
  fn sync_with_probe(&self) {
    if self.handle.is_terminated() {
      panic!("underlying actor is terminated");
    }
    let sequence = self.handle.send_probe(&self.actor_ref);
    if self.handle.is_terminated() {
      panic!("underlying actor is terminated");
    }
    let started = Instant::now();
    while self.handle.processed_sequence() < sequence && !self.handle.is_terminated() {
      if started.elapsed() > PROBE_TIMEOUT {
        panic!("timeout while waiting for the actor at {} to process a probe", self.actor_ref.path());
      }
      if !self.system.run_one() {
        std::thread::sleep(Duration::from_millis(1));
      }
    }
    if self.handle.is_terminated() {
      panic!("underlying actor is terminated");
    }
  }
}

impl core::fmt::Display for TestActorRef {
  /// Mirrors `TestActorRef.toString`, which is `TestActor[<path>]`.
  fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(formatter, "TestActor[{}]", self.actor_ref.path())
  }
}
