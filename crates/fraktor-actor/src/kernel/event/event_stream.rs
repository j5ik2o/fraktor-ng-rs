// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2023 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2019-2023 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The stream that delivers events to their subscribers.

use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::any::Any;
use core::cell::RefCell;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

use crate::kernel::actor::Logging::Debug;
use crate::kernel::actor::Logging::InitializeLogger;
use crate::kernel::actor::Logging::LogLevel;
use crate::kernel::actor::Terminated;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::actor_system::WeakActorSystem;
use crate::kernel::event::event_stream_bus::EventStreamBusWeak;
use crate::kernel::event::EventClass;
use crate::kernel::event::EventStreamBus;
use crate::kernel::event::NullSubscriber;
use crate::kernel::message::Message;

/// The source that the stream reports in its own debug events.
const LOG_SOURCE: &str = "EventStream";

/// The class the ported debug events report, standing in for the `Class` of the
/// stream that Pekko names.
const DEBUG_LOG_CLASS: i32 = 0;

/// The name of the handle that watches the subscribers of the stream.
const UNSUBSCRIBER_NAME: &str = "eventStreamUnsubscriber";

/// The interval that names one unsubscriber apart from the next.
static UNSUBSCRIBER_ID: AtomicUsize = AtomicUsize::new(0);

/// The name of the handle that turns unhandled messages into debug events.
const UNHANDLED_FORWARDER_NAME: &str = "UnhandledMessageForwarder";

/// The event stream of an actor system.
///
/// Mirrors `org.apache.pekko.event.EventStream`: a subscriber registers for a
/// channel and receives every event whose class is that channel or a declared
/// subclass of it. A subscription to [`EventClass::any`] receives every event.
/// Cloning the handle shares the subscribers, as reaching the stream of the
/// system does in Pekko.
///
/// The stream is a value of the system rather than an actor. A publish hands
/// the concrete event to each subscriber through its handle, so a subscriber
/// can recover the event's type, which is what a test probe does with the debug
/// events of an event bus.
///
/// Besides the subscription table, the stream owns the pieces that Pekko starts
/// during `ActorSystem` start-up: the unsubscriber that watches subscribers, the
/// default loggers and their level, and the forwarder for unhandled messages.
/// The port starts them on request, because the goal does not read the
/// configuration text that turns them on.
#[derive(Clone)]
pub struct EventStream {
  system: WeakActorSystem,
  bus: EventStreamBus,
  debug: bool,
  setup: Rc<RefCell<StreamSetup>>,
}

impl EventStream {
  /// Creates an empty stream for `system`.
  ///
  /// Mirrors `EventStream(sys, debug)`: `debug` decides whether the stream
  /// publishes a debug event for the life cycle of its subscribers.
  pub fn new(system: WeakActorSystem, debug: bool) -> Self {
    Self { system, bus: EventStreamBus::new(), debug, setup: Rc::new(RefCell::new(StreamSetup::new())) }
  }

  /// Publishes `event` to every subscriber of the class of `E`.
  ///
  /// Mirrors `EventStream.publish`: each matching subscriber receives the
  /// event. The event keeps its concrete type on the way, so a subscriber can
  /// name `E` and recover it.
  pub fn publish<E: Any + Send + Sync>(&self, event: E) {
    self.bus.publish(event);
  }

  /// Keeps callbacks from extending the lifetime of this stream's subscribers.
  pub(crate) fn downgrade(&self) -> EventStreamBusWeak {
    self.bus.downgrade()
  }

  /// Subscribes `subscriber` to `event_class`.
  ///
  /// Mirrors `EventStream.subscribe`: the first subscription answers `true` and
  /// a repeated one answers `false` without registering again. With debugging
  /// on it publishes the subscription as a debug event and starts watching the
  /// subscriber, so the subscription ends when the subscriber terminates.
  ///
  /// # Errors
  ///
  /// Returns [`NullSubscriber`] when `subscriber` is `None`, which is the
  /// `IllegalArgumentException` Pekko throws for a null subscriber.
  pub fn subscribe(&self, subscriber: Option<ActorRef>, event_class: EventClass) -> Result<bool, NullSubscriber> {
    let Some(subscriber) = subscriber else {
      return Err(NullSubscriber);
    };
    if self.debug {
      let subscribing = format!("subscribing {} to channel {}", subscriber.path(), event_class);
      self.publish(Debug::new(String::from(LOG_SOURCE), DEBUG_LOG_CLASS, &subscribing));
    }
    let added = self.bus.subscribe(Some(subscriber.clone()), event_class)?;
    self.register_with_unsubscriber(&subscriber);
    Ok(added)
  }

  /// Removes the subscription of `subscriber` from `event_class`.
  ///
  /// Mirrors `EventStream.unsubscribe`: the call answers whether the table
  /// changed, publishes the removal as a debug event, and stops watching the
  /// subscriber when it no longer holds any subscription.
  ///
  /// # Errors
  ///
  /// Returns [`NullSubscriber`] when `subscriber` is `None`.
  pub fn unsubscribe(&self, subscriber: Option<ActorRef>, event_class: EventClass) -> Result<bool, NullSubscriber> {
    let Some(subscriber) = subscriber else {
      return Err(NullSubscriber);
    };
    let changed = self.bus.unsubscribe(&subscriber, event_class);
    if self.debug {
      let unsubscribing = format!("unsubscribing {} from channel {}", subscriber.path(), event_class);
      self.publish(Debug::new(String::from(LOG_SOURCE), DEBUG_LOG_CLASS, &unsubscribing));
    }
    self.unregister_if_no_more_subscribed_channels(&subscriber);
    Ok(changed)
  }

  /// Removes every subscription of `subscriber`.
  ///
  /// Mirrors `EventStream.unsubscribe(subscriber)`: it publishes the removal,
  /// then stops watching the subscriber when nothing is left.
  ///
  /// # Errors
  ///
  /// Returns [`NullSubscriber`] when `subscriber` is `None`.
  pub fn unsubscribe_all(&self, subscriber: Option<ActorRef>) -> Result<(), NullSubscriber> {
    let Some(subscriber) = subscriber else {
      return Err(NullSubscriber);
    };
    self.bus.unsubscribe_all(&subscriber);
    if self.debug {
      let unsubscribing = format!("unsubscribing {} from all channels", subscriber.path());
      self.publish(Debug::new(String::from(LOG_SOURCE), DEBUG_LOG_CLASS, &unsubscribing));
    }
    self.unregister_if_no_more_subscribed_channels(&subscriber);
    Ok(())
  }

  /// Records that `Child` is a subclass of `Parent`.
  ///
  /// Rust has no run-time class hierarchy, so a test declares the links that
  /// Pekko reads from `Class.isAssignableFrom`.
  pub fn declare_subclass<Child: 'static, Parent: 'static>(&self) {
    self.bus.declare_subclass(EventClass::of::<Child>(), EventClass::of::<Parent>());
  }

  /// Starts the handle that watches the subscribers of the stream.
  ///
  /// Mirrors `EventStream.startUnsubscriber`: the handle receives a
  /// `Terminated` for each subscriber it watches and removes that subscriber
  /// from the stream. Starting it a second time changes nothing.
  pub fn start_unsubscriber(&self) {
    if self.setup.borrow().unsubscriber.is_some() {
      return;
    }
    let bus = self.bus.clone();
    let debug = self.debug;
    let name = format!("{UNSUBSCRIBER_NAME}-{}", UNSUBSCRIBER_ID.fetch_add(1, Ordering::Relaxed));
    let unsubscriber = self.system.function_ref(&name, move |_sender: &ActorRef, message: Message| {
      let Some(terminated) = message.downcast_ref::<Terminated>() else {
        return;
      };
      let target = terminated.target().clone();
      bus.unsubscribe_all(&target);
      if debug {
        let unsubscribing = format!("unsubscribing {} from all channels", target.path());
        bus.publish(Debug::new(String::from(UNSUBSCRIBER_NAME), DEBUG_LOG_CLASS, &unsubscribing));
      }
      if debug && !bus.has_subscriptions(&target) {
        let unwatching = format!("unwatching {}, since has no subscriptions", target.path());
        bus.publish(Debug::new(String::from(UNSUBSCRIBER_NAME), DEBUG_LOG_CLASS, &unwatching));
      }
    });
    self.activate_unsubscriber(unsubscriber);
  }

  /// Uses `unsubscriber` as the handle that watches subscribers.
  ///
  /// Mirrors `EventStream.initUnsubscriber`: it answers `true` the first time
  /// and `false` when a handle is already installed, which is the state the
  /// double-initialization case observes.
  pub fn init_unsubscriber(&self, unsubscriber: ActorRef) -> bool {
    if self.setup.borrow().unsubscriber.is_some() {
      return false;
    }
    self.activate_unsubscriber(unsubscriber);
    true
  }

  /// Starts `loggers` and makes them listen to `level` and every more severe
  /// level.
  ///
  /// Mirrors `LoggingBus.startDefaultLoggers`: each logger receives
  /// `InitializeLogger` and its subscription is set to exactly the channels of
  /// the accepted levels, and `level` is remembered so a later
  /// [`EventStream::set_log_level`] can change the set. The accepted channels
  /// are subscribed and the channels `level` does not accept are unsubscribed,
  /// which is the same dropping side [`EventStream::set_log_level`] uses, so
  /// restarting an already started logger at a lower level leaves the
  /// subscription state a fresh start at that level would have. Applying the
  /// absolute set rather than [`EventStream::set_log_level`]'s difference from
  /// the saved level keeps a `set_log_level` made before any logger started
  /// from leaving the loggers subscribed to nothing.
  pub fn start_default_loggers(&self, loggers: &[ActorRef], level: LogLevel) {
    self.setup.borrow_mut().loggers = loggers.to_vec();
    for logger in loggers {
      logger.tell(InitializeLogger::new(self.bus.clone()), logger.clone());
    }
    self.system.run_until_idle();
    for candidate in LOG_LEVELS {
      let class = candidate.event_class();
      if candidate <= level {
        for logger in loggers {
          let _ = self.subscribe(Some(logger.clone()), class);
        }
      } else {
        for logger in loggers {
          let _ = self.unsubscribe(Some(logger.clone()), class);
        }
      }
    }
    self.setup.borrow_mut().log_level = Some(level);
  }

  /// Changes the level that the default loggers listen to.
  ///
  /// Mirrors `LoggingBus.setLogLevel`: a level the loggers did not listen to
  /// before and that the new setting accepts is subscribed, and a level they
  /// listened to and the new setting drops is unsubscribed. `Error < Warning <
  /// Info < Debug`, so a higher setting also accepts the less severe levels.
  pub fn set_log_level(&self, level: LogLevel) {
    let (previous, loggers) = {
      let setup = self.setup.borrow();
      (setup.log_level, setup.loggers.clone())
    };
    for candidate in LOG_LEVELS {
      let class = candidate.event_class();
      if previous.is_none_or(|old| candidate > old) && candidate <= level {
        for logger in &loggers {
          let _ = self.subscribe(Some(logger.clone()), class);
        }
      }
      if previous.is_some_and(|old| candidate <= old) && candidate > level {
        for logger in &loggers {
          let _ = self.unsubscribe(Some(logger.clone()), class);
        }
      }
    }
    self.setup.borrow_mut().log_level = Some(level);
  }

  /// Starts the handle that turns unhandled messages into debug events.
  ///
  /// Mirrors the `UnhandledMessageForwarder` that
  /// `LoggingBus.startDefaultLoggers` installs: it subscribes to
  /// [`UnhandledMessage`](crate::kernel::actor::UnhandledMessage) and publishes
  /// a `Debug` that names the sender, the message, and the recipient.
  pub fn start_unhandled_message_forwarder(&self) {
    if self.setup.borrow().forwarder_started {
      return;
    }
    self.setup.borrow_mut().forwarder_started = true;
    let bus = self.bus.downgrade();
    let forwarder = self.system.function_ref(UNHANDLED_FORWARDER_NAME, move |_sender: &ActorRef, message: Message| {
      let Some(unhandled) = message.downcast_ref::<crate::kernel::actor::UnhandledMessage>() else {
        return;
      };
      let source = unhandled.recipient().path().to_string();
      let text = format!("unhandled message from {}: {:?}", unhandled.sender().path(), unhandled.message());
      bus.publish(Debug::new(source, DEBUG_LOG_CLASS, &text));
    });
    let _ = self.bus.subscribe(Some(forwarder), EventClass::of::<crate::kernel::actor::UnhandledMessage>());
  }

  fn activate_unsubscriber(&self, unsubscriber: ActorRef) {
    self.setup.borrow_mut().unsubscriber = Some(unsubscriber);
    for subscriber in self.bus.subscribers() {
      self.register_with_unsubscriber(&subscriber);
    }
  }

  fn register_with_unsubscriber(&self, subscriber: &ActorRef) {
    let unsubscriber = self.setup.borrow().unsubscriber.clone();
    let Some(unsubscriber) = unsubscriber else {
      return;
    };
    if self.debug {
      let watching =
        format!("watching {} in order to unsubscribe from EventStream when it terminates", subscriber.path());
      self.publish(Debug::new(String::from(UNSUBSCRIBER_NAME), DEBUG_LOG_CLASS, &watching));
    }
    self.system.watch(&unsubscriber, subscriber);
  }

  fn unregister_if_no_more_subscribed_channels(&self, subscriber: &ActorRef) {
    if self.bus.has_subscriptions(subscriber) {
      return;
    }
    let unsubscriber = self.setup.borrow().unsubscriber.clone();
    let Some(unsubscriber) = unsubscriber else {
      return;
    };
    if self.debug {
      let unwatching = format!("unwatching {}, since has no subscriptions", subscriber.path());
      self.publish(Debug::new(String::from(UNSUBSCRIBER_NAME), DEBUG_LOG_CLASS, &unwatching));
    }
    self.system.unwatch(&unsubscriber, subscriber);
  }
}

/// The pieces the stream starts on request, besides its subscribers.
struct StreamSetup {
  unsubscriber: Option<ActorRef>,
  loggers: Vec<ActorRef>,
  log_level: Option<LogLevel>,
  forwarder_started: bool,
}

impl StreamSetup {
  fn new() -> Self {
    Self { unsubscriber: None, loggers: Vec::new(), log_level: None, forwarder_started: false }
  }
}

/// The levels of a log event, from the most severe to the least.
const LOG_LEVELS: [LogLevel; 4] = [LogLevel::Error, LogLevel::Warning, LogLevel::Info, LogLevel::Debug];
