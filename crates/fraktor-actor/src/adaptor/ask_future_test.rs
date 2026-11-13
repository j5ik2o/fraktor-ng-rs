//! Unit tests for [`AskFuture`](super::AskFuture).

use std::sync::Arc;
use std::sync::Mutex;

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor::{Actor, ActorContext, Props};
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::actor_system::ActorSystem;
use crate::kernel::message::Message;

use super::AskFuture;

/// An actor that ignores every message, so an `ask` it receives never replies.
struct SilentActor;

impl Actor for SilentActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// Collects what a function reference receives, so a test can read it back.
#[derive(Clone, Default)]
struct Sink {
  received: Arc<Mutex<Vec<Message>>>,
}

impl Sink {
  fn record(&self, message: Message) {
    self.received.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(message);
  }

  fn take(&self) -> Vec<Message> {
    core::mem::take(&mut self.received.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
  }
}

/// Shares the sender an `ask` used, so a test can reply to it.
#[derive(Clone, Default)]
struct AskSender {
  sender: Arc<Mutex<Option<ActorRef>>>,
}

impl AskSender {
  fn set(&self, sender: &ActorRef) {
    *self.sender.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sender.clone());
  }

  fn take(&self) -> ActorRef {
    self
      .sender
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .clone()
      .expect("the responder should have captured the ask sender")
  }
}

fn system() -> ActorSystem {
  ActorSystem::new("AskFutureSpec", Arc::new(ChannelBoundary::new()))
}

#[test]
fn pipe_to_before_the_reply_sends_the_reply_once_it_arrives() {
  let mut system = system();

  let ask_sender = AskSender::default();
  let captured = ask_sender.clone();
  let responder = system.function_ref("responder", move |sender: &ActorRef, _message: Message| captured.set(sender));

  let sink = Sink::default();
  let recorder = sink.clone();
  let recipient =
    system.function_ref("recipient", move |_sender: &ActorRef, message: Message| recorder.record(message));

  let future: AskFuture = responder.ask("question");

  future.pipe_to(recipient);

  let sender = ask_sender.take();
  sender.tell("answer", sender.clone());

  let received = sink.take();
  assert_eq!(received.len(), 1, "the reply should be piped exactly once");
  assert_eq!(received[0], "answer");
}

#[test]
fn pipe_to_after_the_reply_sends_the_reply_at_once() {
  let mut system = system();

  let ask_sender = AskSender::default();
  let captured = ask_sender.clone();
  let responder = system.function_ref("responder", move |sender: &ActorRef, _message: Message| captured.set(sender));

  let sink = Sink::default();
  let recorder = sink.clone();
  let recipient =
    system.function_ref("recipient", move |_sender: &ActorRef, message: Message| recorder.record(message));

  let future = responder.ask("question");
  let sender = ask_sender.take();
  sender.tell("answer", sender.clone());
  assert!(future.is_completed(), "the reply should have arrived before pipe_to");

  future.pipe_to(recipient);

  let received = sink.take();
  assert_eq!(received.len(), 1, "the reply should be piped exactly once");
  assert_eq!(received[0], "answer");
}

#[test]
fn a_reply_to_the_piped_sender_is_dropped_and_the_result_is_unchanged() {
  let mut system = system();

  let responder_sink = Sink::default();
  let responder_recorder = responder_sink.clone();
  let ask_sender = AskSender::default();
  let captured = ask_sender.clone();
  let responder = system.function_ref("responder", move |sender: &ActorRef, message: Message| {
    responder_recorder.record(message);
    captured.set(sender);
  });

  let recipient = system.function_ref("recipient", move |sender: &ActorRef, _message: Message| {
    sender.tell("reply-to-pipe", sender.clone());
  });

  let future = responder.ask("question");
  let sender = ask_sender.take();
  sender.tell("answer", sender.clone());

  future.pipe_to(recipient);

  assert_eq!(
    responder_sink.take().len(),
    1,
    "the responder should receive only the original question, never the piped reply"
  );
  assert_eq!(future.await_result(core::time::Duration::from_secs(1)), "answer");
}

#[test]
fn an_unanswered_ask_on_a_registered_driver_times_out_within_max() {
  let mut system = system();
  // A driver is registered for this shard, so the wait drives the shard
  // instead of only blocking on the condition variable.
  crate::adaptor::attach_wait_driver(&system);
  let silent = system.actor_of(Props::new(|| SilentActor));

  let max = core::time::Duration::from_millis(300);
  let started = std::time::Instant::now();
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| silent.ask("question").await_result(max)));
  let elapsed = started.elapsed();

  assert!(outcome.is_err(), "an unanswered ask should panic with the timeout");
  // `drive_until` spends up to `max` driving the shard; the fallback wait must
  // reuse that budget, so the total stays close to `max`, not `2 * max`.
  assert!(elapsed < max + core::time::Duration::from_millis(250), "waited {elapsed:?}, more than about one {max:?}");
}
