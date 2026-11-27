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

//! The state machine that logs dead letters up to a limit, then suspends.

#[cfg(test)]
#[path = "dead_letter_listener_test.rs"]
mod tests;

use alloc::format;
use alloc::string::String;
use core::time::Duration;

use crate::kernel::actor::AnyDeadLetter;
use crate::kernel::actor::DeadLetter;
use crate::kernel::actor::DeadLetterLogLimit;
use crate::kernel::actor::DeadLetterSettings;
use crate::kernel::actor::SuppressedDeadLetter;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;
use crate::kernel::pretty_duration::PrettyDurationInput;

/// The counting and suspension state of the dead letter listener.
///
/// Mirrors `org.apache.pekko.event.DeadLetterListener` without the actor
/// around it: the executor starts a function reference whose handler feeds
/// every event the listener subscribes to into [`Self::on_event`], and
/// publishes the `Info` the call returns, if any, to the event stream and
/// the event sink. Keeping the state machine apart from the handler makes
/// it testable on its own, which the property tests use.
///
/// The listener counts `DeadLetter`, `Dropped`, and `UnhandledMessage`
/// events. It never counts a `SuppressedDeadLetter`, and it does not count
/// a `DeadLetter` whose own message is a suppressed dead letter event —
/// Pekko's `isWrappedSuppressed`. A direct suppressed payload inside a
/// `DeadLetter` is counted, as in Pekko.
pub struct DeadLetterListener {
  settings: DeadLetterSettings,
  dead_letters: ActorRef,
  count: u32,
  suspended_until: Option<Duration>,
  stopped: bool,
}

impl DeadLetterListener {
  /// Creates the listener state for `settings`, naming `dead_letters` as
  /// the handle that stands for "no real sender" in the log lines.
  pub fn new(settings: DeadLetterSettings, dead_letters: ActorRef) -> Self {
    Self { settings, dead_letters, count: 0, suspended_until: None, stopped: false }
  }

  /// Returns whether the listener has stopped itself: the count reached
  /// the limit while no suspend duration was set, which is where Pekko's
  /// listener stops its own actor. The handler reads this right after
  /// feeding the event that stopped the machine and posts the stop
  /// notice to the shard boundary, because the handler must not borrow
  /// the executor; the delivery of the notice carries the stop out.
  pub fn is_stopped(&self) -> bool {
    self.stopped
  }

  /// Feeds one event at time `now` and returns the log line to publish, if
  /// the event is logged.
  ///
  /// `is_suppressed` answers whether a payload is marked for suppression;
  /// the caller passes the routing table's predicate so this state machine
  /// stays free of the registry.
  pub fn on_event(
    &mut self,
    event: &AnyDeadLetter<'_>,
    now: Duration,
    is_suppressed: &dyn Fn(&Message) -> bool,
  ) -> Option<String> {
    if self.stopped {
      return None;
    }
    if matches!(event, AnyDeadLetter::Suppressed(_)) {
      return None;
    }
    if let AnyDeadLetter::DeadLetter(dead) = event {
      if is_wrapped_suppressed(dead, is_suppressed) {
        return None;
      }
    }
    if self.settings.limit() == DeadLetterLogLimit::Off {
      return None;
    }
    // The count saturates instead of wrapping: wrapping to 1 inside a
    // suspension would make the resume line's `count - limit - 1`
    // underflow, and a saturated count stays at or above the limit the
    // suspension began at, so the subtraction cannot underflow.
    self.count = self.count.saturating_add(1);
    if let Some(until) = self.suspended_until {
      if now > until {
        let limit = limit_value(self.settings.limit());
        let not_logged = self.count - limit - 1;
        let done = format!(", of which {not_logged} were not logged. The counter will be reset now");
        let text = self.log_line(event, &done);
        self.count = 0;
        self.suspended_until = None;
        return Some(text);
      }
      return None;
    }
    match self.settings.limit() {
      | DeadLetterLogLimit::Unlimited => Some(self.log_line(event, "")),
      | DeadLetterLogLimit::Count(limit) => {
        if self.count == limit.get() {
          match self.settings.suspend_duration() {
            | Some(duration) => {
              let done = format!(
                ", no more dead letters will be logged in next [{}]",
                PrettyDurationInput::Finite(duration).pretty()
              );
              let text = self.log_line(event, &done);
              self.suspended_until = Some(now + duration);
              Some(text)
            },
            | None => {
              let text = self.log_line(event, ", no more dead letters will be logged");
              self.stopped = true;
              Some(text)
            },
          }
        } else {
          Some(self.log_line(event, ""))
        }
      },
      | DeadLetterLogLimit::Off => None,
    }
  }

  fn log_line(&self, event: &AnyDeadLetter<'_>, done: &str) -> String {
    let (type_name, wrapped_in) = unwrapped(event.message());
    let origin = if self.is_real(event.sender()) { format!(" from {}", event.sender().path()) } else { String::new() };
    let body = match event {
      | AnyDeadLetter::Dropped(dropped) => {
        let destination =
          if self.is_real(event.recipient()) { format!(" to {}", event.recipient().path()) } else { String::new() };
        format!(
          "Message [{type_name}]{wrapped_in}{origin}{destination} was dropped. {}. [{}] dead letters encountered{done}. ",
          dropped.reason(),
          self.count
        )
      },
      | AnyDeadLetter::Unhandled(_) => {
        let destination =
          if self.is_real(event.recipient()) { format!(" to {}", event.recipient().path()) } else { String::new() };
        format!(
          "Message [{type_name}]{wrapped_in}{origin}{destination} was unhandled. [{}] dead letters encountered{done}. ",
          self.count
        )
      },
      | _ => {
        format!(
          "Message [{type_name}]{wrapped_in}{origin} to {} was not delivered. [{}] dead letters encountered{done}. If this is not an expected behavior then {} may have terminated unexpectedly. ",
          event.recipient().path(),
          self.count,
          event.recipient().path()
        )
      },
    };
    format!(
      "{body}This logging can be turned off or adjusted with configuration settings 'pekko.log-dead-letters' and 'pekko.log-dead-letters-during-shutdown'."
    )
  }

  fn is_real(&self, reference: &ActorRef) -> bool {
    reference != &self.dead_letters
  }
}

/// Returns whether `dead` wraps a suppressed dead letter event.
///
/// Mirrors Pekko's `isWrappedSuppressed`: only a wrapped message counts —
/// a `DeadLetter` whose own message is a `SuppressedDeadLetter`, or whose
/// own message is a `DeadLetter` carrying a suppressed payload. A
/// `DeadLetter` that directly carries a suppressed payload is not wrapped
/// and is counted.
fn is_wrapped_suppressed(dead: &DeadLetter, is_suppressed: &dyn Fn(&Message) -> bool) -> bool {
  if dead.message().downcast_ref::<SuppressedDeadLetter>().is_some() {
    return true;
  }
  if let Some(inner) = dead.message().downcast_ref::<DeadLetter>() {
    return is_suppressed(inner.message());
  }
  false
}

/// Returns the type name a log line names for `message`, unwrapping one
/// dead letter layer, and the "wrapped in" suffix for a wrapped message.
fn unwrapped(message: &Message) -> (String, String) {
  if let Some(inner) = message.downcast_ref::<DeadLetter>() {
    return (
      String::from(inner.message().payload_type_name()),
      format!(" wrapped in [{}]", message.payload_type_name()),
    );
  }
  if let Some(inner) = message.downcast_ref::<SuppressedDeadLetter>() {
    return (
      String::from(inner.message().payload_type_name()),
      format!(" wrapped in [{}]", message.payload_type_name()),
    );
  }
  (String::from(message.payload_type_name()), String::new())
}

fn limit_value(limit: DeadLetterLogLimit) -> u32 {
  match limit {
    | DeadLetterLogLimit::Count(count) => count.get(),
    | DeadLetterLogLimit::Unlimited => u32::MAX,
    | DeadLetterLogLimit::Off => 0,
  }
}
