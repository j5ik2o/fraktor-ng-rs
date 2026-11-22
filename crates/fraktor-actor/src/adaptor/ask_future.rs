//! The future that an `ask` completes and the `ask` that creates it.

use alloc::sync::Arc;
use core::fmt;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::sync::Condvar;
use std::sync::Mutex;
use std::time::Instant;

use crate::adaptor::AskError;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::next_actor_ref;
use crate::kernel::executor::FunctionBoundary;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Message;
use crate::kernel::timeout::Timeout;

#[cfg(test)]
#[path = "ask_future_test.rs"]
mod tests;

/// The counter that names each temporary reply handle apart from the next.
static NEXT_REPLIER: AtomicU64 = AtomicU64::new(0);

/// Owns the ask deadline, its terminal result, and pending success recipients.
///
/// Keeping them under one lock is what makes `pipe_to` and the
/// reply order-independent: whichever arrives second sees the first and makes
/// the send happen exactly once.
struct AskState {
  deadline: Option<Instant>,
  outcome: Option<AskOutcome>,
  pending: Vec<PipedRecipient>,
}

enum AskOutcome {
  Reply { message: Message, received_at: Instant },
  TimedOut,
}

/// A recipient waiting for the reply, with the sender to send it from.
struct PipedRecipient {
  recipient: ActorRef,
  sender: ActorRef,
}

impl AskState {
  fn new(deadline: Option<Instant>) -> Self {
    Self { deadline, outcome: None, pending: Vec::new() }
  }

  fn outcome_at(&mut self, now: Instant) -> Option<&AskOutcome> {
    if self.outcome.is_none() && self.deadline.is_some_and(|deadline| now >= deadline) {
      self.outcome = Some(AskOutcome::TimedOut);
      self.pending.clear();
    }
    self.outcome.as_ref()
  }

  fn wait_deadline(&self, outer: Instant) -> Instant {
    self.deadline.map_or(outer, |deadline| deadline.min(outer))
  }

  fn result_for_wait(&mut self, outer: Instant) -> Option<Result<Message, AskError>> {
    let deadline = self.deadline;
    match self.outcome_at(Instant::now())? {
      // A single driver step can overrun both deadlines. Preserve which
      // deadline came first without changing the ask's terminal result.
      | AskOutcome::TimedOut if deadline.is_some_and(|deadline| outer < deadline) => Some(Err(AskError::WaitTimedOut)),
      | AskOutcome::TimedOut => Some(Err(AskError::AskTimedOut)),
      // A deadline-bound ask may accept a reply after this wait has expired.
      // Keep the reply available to any later wait.
      | AskOutcome::Reply { received_at, .. } if deadline.is_some() && *received_at > outer => {
        Some(Err(AskError::WaitTimedOut))
      },
      | AskOutcome::Reply { message, .. } => Some(Ok(message.clone())),
    }
  }

  /// Records the first reply and returns the recipients waiting for it.
  ///
  /// Later replies are dropped, matching Pekko's future, which completes once.
  fn complete(&mut self, message: Message) -> Vec<PipedRecipient> {
    let received_at = Instant::now();
    if self.outcome_at(received_at).is_some() {
      return Vec::new();
    }
    self.outcome = Some(AskOutcome::Reply { message, received_at });
    core::mem::take(&mut self.pending)
  }

  /// Returns the reply if it has already arrived, otherwise queues `recipient`.
  fn pipe(&mut self, recipient: ActorRef, sender: ActorRef) -> Option<Message> {
    match self.outcome_at(Instant::now()) {
      | Some(AskOutcome::Reply { message, .. }) => Some(message.clone()),
      | Some(AskOutcome::TimedOut) => None,
      | None => {
        self.pending.push(PipedRecipient { recipient, sender });
        None
      },
    }
  }
}

/// The outcome of an `ask`, readable from the thread that waits for it.
///
/// Mirrors `org.apache.pekko.pattern.ask`, which pairs each question with a
/// temporary actor and completes a `Future` when the reply arrives. The port
/// uses a temporary function reference as the sender and stores the reply in a
/// slot that the waiting thread observes. The slot lives in the adaptor
/// because waiting across threads needs `Mutex` and `Condvar`, which the
/// kernel may not use.
pub struct AskFuture {
  slot: Arc<(Mutex<AskState>, Condvar)>,
  driver_key: usize,
}

impl AskFuture {
  fn new(slot: Arc<(Mutex<AskState>, Condvar)>, driver_key: usize) -> Self {
    Self { slot, driver_key }
  }

  /// Returns whether a reply or the ask's own deadline has completed the ask.
  pub fn is_completed(&self) -> bool {
    lock(&self.slot.0).outcome_at(Instant::now()).is_some()
  }

  /// Waits for the reply using `max` as the waiting limit.
  ///
  /// # Panics
  ///
  /// Panics if the wait or the ask expires before a reply is returned.
  pub fn await_result(&self, max: Duration) -> Message {
    match self.try_await_result(max) {
      | Ok(message) => message,
      | Err(AskError::WaitTimedOut) => panic!("timeout ({max:?}) during ask while waiting for a reply"),
      | Err(AskError::AskTimedOut) => panic!("ask deadline expired while waiting for a reply"),
    }
  }

  /// Waits for a reply and distinguishes the ask deadline from the wait limit.
  ///
  /// A wait timeout does not cancel the ask. When an ask has its own deadline,
  /// a reply that arrives after this wait's limit can still be read by a later wait.
  /// If this wait drives the actor, it can only return after the current step finishes.
  pub fn try_await_result(&self, max: Duration) -> Result<Message, AskError> {
    let deadline = Instant::now() + max;
    crate::adaptor::ask_driver::drive_until(
      self.driver_key,
      deadline.saturating_duration_since(Instant::now()),
      || self.is_completed(),
    );
    let mut guard = lock(&self.slot.0);
    loop {
      if let Some(result) = guard.result_for_wait(deadline) {
        return result;
      }
      let now = Instant::now();
      if now >= deadline {
        return Err(AskError::WaitTimedOut);
      }
      let remaining = guard.wait_deadline(deadline).saturating_duration_since(now);
      let (next, _) = self.slot.1.wait_timeout(guard, remaining).unwrap_or_else(|poisoned| poisoned.into_inner());
      guard = next;
    }
  }

  /// Sends the reply to `recipient` once it arrives.
  ///
  /// Mirrors `org.apache.pekko.pattern.PipeToSupport.pipeTo`: the reply is
  /// forwarded from `recipient`'s perspective as a message, and a reply that
  /// already arrived is sent at once. Pekko defaults the sender to
  /// `ActorRef.noSender`, so a recipient that replies reaches dead letters; the
  /// port uses a no-op reference for the same effect.
  pub fn pipe_to(&self, recipient: ActorRef) {
    let sender = recipient.replier("pipe", |_sender: &ActorRef, _message: Message| {});
    let message = lock(&self.slot.0).pipe(recipient.clone(), sender.clone());
    if let Some(message) = message {
      recipient.tell(message, sender);
    }
  }
}

impl fmt::Debug for AskFuture {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.debug_struct("AskFuture").field("completed", &self.is_completed()).finish()
  }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
  mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl ActorRef {
  /// Sends `message` and waits for the first reply addressed to the sender.
  ///
  /// Mirrors `org.apache.pekko.pattern.AskableActorRef.?`: a temporary reply
  /// handle is the sender, the question is sent with it, and the returned
  /// future completes when the actor replies to that sender. The temporary
  /// handle is a function reference, so the reply fills the slot before the
  /// replying actor returns, which is why the caller can observe completion
  /// straight after the send.
  pub fn ask<M: core::any::Any + Send + Sync>(&self, message: M) -> AskFuture {
    self.start_ask(message, None)
  }

  /// Sends a question whose result expires after `timeout`, independently
  /// of how long any caller waits for it.
  pub fn ask_with_timeout<M: core::any::Any + Send + Sync>(&self, message: M, timeout: Timeout) -> AskFuture {
    self.start_ask(message, Some(Instant::now() + timeout.duration()))
  }

  fn start_ask<M: core::any::Any + Send + Sync>(&self, message: M, deadline: Option<Instant>) -> AskFuture {
    let slot = Arc::new((Mutex::new(AskState::new(deadline)), Condvar::new()));
    let reply = slot.clone();
    let handler = move |_sender: &ActorRef, reply_message: Message| {
      let pending = {
        let mut guard = lock(&reply.0);
        let pending = guard.complete(reply_message.clone());
        reply.1.notify_all();
        pending
      };
      for PipedRecipient { recipient, sender } in pending {
        recipient.tell(reply_message.clone(), sender);
      }
    };
    let replier = self.replier("ask", handler);
    let driver_key = self.boundary_key();
    self.tell(Message::new(message), replier);
    AskFuture::new(slot, driver_key)
  }

  fn replier<H>(&self, tag: &str, handler: H) -> ActorRef
  where
    H: Fn(&ActorRef, Message) + Send + Sync + 'static,
  {
    let name = alloc::format!("{tag}-${}", NEXT_REPLIER.fetch_add(1, Ordering::Relaxed));
    let root = self.path().root().clone();
    let path = ActorPath::from_root(root).with_child("temp").with_child(&name);
    let boundary: Arc<dyn ShardBoundary> = Arc::new(FunctionBoundary::new(Arc::new(handler)));
    next_actor_ref(path, boundary)
  }
}
