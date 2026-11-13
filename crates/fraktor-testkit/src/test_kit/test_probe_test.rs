//! Unit tests for [`TestProbe`](super::TestProbe).

use std::panic::catch_unwind;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

use super::TestProbe;
use crate::test_kit::AutoPilot;
use crate::test_kit::AutoPilotControl;

fn failure_text(failure: Box<dyn core::any::Any + Send>) -> String {
  match failure.downcast::<String>() {
    | Ok(text) => *text,
    | Err(failure) => failure.downcast::<&'static str>().map(|text| text.to_string()).unwrap_or_default(),
  }
}

#[test]
fn fish_for_message_fails_when_no_message_matches_in_time() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  let mut probe = TestProbe::new(&mut system);
  probe.ref_handle().tell("other", probe.ref_handle());

  let failure = catch_unwind(AssertUnwindSafe(|| {
    probe.fish_for_message(Duration::from_millis(50), |message: &Message| message == *"wanted");
  }))
  .expect_err("fish_for_message returned although nothing matched");

  assert!(failure_text(failure).contains("during fishForMessage"));
}

#[test]
fn fish_for_specific_message_fails_when_no_message_matches_in_time() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  let mut probe = TestProbe::new(&mut system);
  probe.ref_handle().tell("other", probe.ref_handle());

  let failure = catch_unwind(AssertUnwindSafe(|| {
    probe.fish_for_specific_message(Duration::from_millis(50), |_: &Message| None::<()>);
  }))
  .expect_err("fish_for_specific_message returned although nothing matched");

  assert!(failure_text(failure).contains("during fishForSpecificMessage"));
}

#[test]
fn fish_for_message_updates_last_message() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  let mut probe = TestProbe::new(&mut system);
  let other = TestProbe::new(&mut system);
  let other_ref = other.ref_handle();
  probe.ref_handle().tell("skip", probe.ref_handle());
  probe.ref_handle().tell("wanted", other_ref.clone());

  let found = probe.fish_for_message(Duration::from_millis(200), |message: &Message| message == *"wanted");

  assert_eq!(found, *"wanted");
  assert_eq!(
    probe.last_message().sender(),
    &other_ref,
    "fish_for_message did not record the sender of the message it accepted"
  );
}

#[test]
#[should_panic(expected = "terminat")]
fn creating_a_probe_after_terminate_is_rejected() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  system.terminate();

  let _probe = TestProbe::new(&mut system);
}

#[test]
fn watching_a_live_probe_receives_no_terminated() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  let mut probe = TestProbe::new(&mut system);
  let other = TestProbe::new(&mut system);
  let other_ref = other.ref_handle();

  probe.watch(&other_ref);
  probe.expect_no_message(Duration::from_millis(50));
}

struct AnswerOnce {
  answered: bool,
}

impl AutoPilot for AnswerOnce {
  fn run(&mut self, sender: &ActorRef, _message: Message) -> AutoPilotControl {
    if !self.answered {
      self.answered = true;
      sender.tell("reply", sender.clone());
    }
    AutoPilotControl::KeepRunning
  }
}

#[test]
fn a_pilot_that_sends_to_its_own_probe_does_not_deadlock() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  let mut probe = TestProbe::new(&mut system);
  let probe_ref = probe.ref_handle();
  probe.set_auto_pilot(AnswerOnce { answered: false });

  probe_ref.tell("go", probe_ref.clone());

  probe.expect_msg(&"go");
  probe.expect_msg(&"reply");
}

/// Counts every message it is given and, on the first two runs, tells the test
/// that it started before waiting for the test to let it go.
///
/// The test sends the next message and waits until it has been handed to the
/// mailbox, then lets the current run finish. The second run observes the
/// outcome; the third is observed through the run count because nothing is sent
/// after it.
struct BlockingPilot {
  runs: Arc<AtomicUsize>,
  started: std::sync::mpsc::Sender<usize>,
  release: std::sync::mpsc::Receiver<()>,
}

impl AutoPilot for BlockingPilot {
  fn run(&mut self, _sender: &ActorRef, _message: Message) -> AutoPilotControl {
    let run = self.runs.fetch_add(1, Ordering::SeqCst) + 1;
    if run <= 2 {
      let _ = self.started.send(run);
      let _ = self.release.recv();
    }
    AutoPilotControl::KeepRunning
  }
}

#[test]
fn a_message_from_another_thread_during_a_pilot_run_is_piloted() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  let mut probe = TestProbe::new(&mut system);
  let probe_ref = probe.ref_handle();
  let runs = Arc::new(AtomicUsize::new(0));
  let (started_sender, started_receiver) = std::sync::mpsc::channel();
  let (release_sender, release_receiver) = std::sync::mpsc::channel();
  probe.set_auto_pilot(BlockingPilot { runs: Arc::clone(&runs), started: started_sender, release: release_receiver });

  let wait = Duration::from_secs(5);

  // Each message is sent from its own thread, because `tell` runs the handler on
  // the sending thread: a thread that holds a pilot run cannot also send the
  // next message. While a run is held, the pilot is not in the mailbox, so a
  // sender only queues its message and returns; `delivered` then means "the
  // message is in the mailbox". The test releases the held run only after that,
  // so the message is certainly queued while the pilot is held. If a sender had
  // stolen the pilot instead of queueing, it would have piloted the message
  // itself, `runs` would already have grown, and the held run would never see
  // it; the `runs` checks below detect exactly that.
  let mut senders = Vec::new();
  let (delivered_sender, delivered_receiver) = std::sync::mpsc::channel();
  let mut send = |text: &'static str| {
    let reference = probe_ref.clone();
    let delivered = delivered_sender.clone();
    senders.push(std::thread::spawn(move || {
      reference.tell(text, reference.clone());
      let _ = delivered.send(());
    }));
  };

  // The first sender is the thread that holds the pilot, so its `tell` does not
  // return until the test releases that run; the pilot's own signal is what
  // shows the message arrived.
  send("first");
  assert_eq!(started_receiver.recv_timeout(wait).expect("the first message never reached the pilot"), 1);

  // "second" is queued while the first run is still held.
  send("second");
  delivered_receiver.recv_timeout(wait).expect("the second message was never handed to the mailbox");
  assert_eq!(runs.load(Ordering::SeqCst), 1, "the sender piloted the message instead of queueing it");
  release_sender.send(()).expect("the test stopped releasing the pilot");
  assert_eq!(started_receiver.recv_timeout(wait).expect("the second message was not piloted by the held run"), 2);

  // "third" is queued while the second run is still held.
  send("third");
  delivered_receiver.recv_timeout(wait).expect("the third message was never handed to the mailbox");
  assert_eq!(runs.load(Ordering::SeqCst), 2, "the sender piloted the message instead of queueing it");
  release_sender.send(()).expect("the test stopped releasing the pilot");

  for sender in senders {
    sender.join().expect("a sending thread panicked");
  }
  assert_eq!(runs.load(Ordering::SeqCst), 3, "a message sent while the pilot ran did not reach the pilot");

  probe.expect_msg_within(wait, &"first");
  probe.expect_msg_within(wait, &"second");
  probe.expect_msg_within(wait, &"third");
}
