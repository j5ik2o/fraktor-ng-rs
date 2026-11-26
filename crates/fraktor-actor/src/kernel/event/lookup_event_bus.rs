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

//! The event bus that looks a classifier up in a table.

use alloc::boxed::Box;
use alloc::vec::Vec;

/// The step that hands one event to one subscriber.
type Deliver<E, S> = Box<dyn Fn(&E, &S)>;

/// An event bus that maps each classifier to the subscribers of that channel.
///
/// Mirrors `org.apache.pekko.event.LookupClassification`: the bus classifies an
/// event once and delivers it to the subscribers the table holds for that
/// classifier. A subscription is a pair of a classifier and a subscriber, and
/// the same pair is refused a second time.
pub struct LookupEventBus<E, C: PartialEq, S: PartialEq> {
  classify: Box<dyn Fn(&E) -> C>,
  publish: Deliver<E, S>,
  subscribers: Vec<(C, S)>,
}

impl<E, C: PartialEq, S: PartialEq> LookupEventBus<E, C, S> {
  /// Creates a bus that classifies with `classify` and delivers with `publish`.
  pub fn new<F, P>(classify: F, publish: P) -> Self
  where
    F: Fn(&E) -> C + 'static,
    P: Fn(&E, &S) + 'static,
  {
    Self { classify: Box::new(classify), publish: Box::new(publish), subscribers: Vec::new() }
  }

  /// Subscribes `subscriber` to `to`.
  ///
  /// Mirrors `EventBus.subscribe`: a new pair answers `true` and one the table
  /// already holds answers `false`.
  pub fn subscribe(&mut self, subscriber: S, to: C) -> bool {
    if self.subscribers.iter().any(|(classifier, existing)| classifier == &to && existing == &subscriber) {
      return false;
    }
    self.subscribers.push((to, subscriber));
    true
  }

  /// Removes the subscription of `subscriber` from `from`.
  ///
  /// Mirrors `EventBus.unsubscribe`: a pair the table holds answers `true` and
  /// one it does not hold answers `false`.
  pub fn unsubscribe(&mut self, subscriber: &S, from: &C) -> bool {
    match self.subscribers.iter().position(|(classifier, existing)| classifier == from && existing == subscriber) {
      | Some(index) => {
        self.subscribers.remove(index);
        true
      },
      | None => false,
    }
  }

  /// Publishes `event` to the subscribers of its classifier.
  pub fn publish(&mut self, event: &E) {
    let classifier = (self.classify)(event);
    for (to, subscriber) in &self.subscribers {
      if *to == classifier {
        (self.publish)(event, subscriber);
      }
    }
  }
}
