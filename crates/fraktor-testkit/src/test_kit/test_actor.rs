//! The actor that collects the messages a test expects.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::MutexGuard;

use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;

use crate::test_kit::AutoPilot;
use crate::test_kit::AutoPilotControl;
use crate::test_kit::ReceivedMessage;

/// The filter that decides which messages the test actor drops.
pub(crate) type Ignore = Box<dyn Fn(&Message) -> bool + Send>;

/// Forwards every message it receives to the queue of the test kit.
///
/// Mirrors `org.apache.pekko.testkit.TestKit.TestActor`. Pekko gives it a
/// dispatcher and a blocking queue; the port keeps the same behaviour in a
/// queue guarded by a mutex, because the test thread reads it while an actor
/// of the system writes it.
///
/// For each message it runs the installed auto pilot first, then consults the
/// ignore filter, and queues the message only when the filter does not drop
/// it. That order matters: a pilot sees every message, while an ignored
/// message never reaches `expectMsg`.
///
/// Pekko's test actor handles one message at a time and gives every message to
/// the pilot. The port gets the same order from [`Mailbox`]: a message that
/// arrives while a pilot runs is left pending and handled by the thread that is
/// already in the actor, so it is piloted too instead of being read as "no
/// pilot". The pilot itself runs outside the mailbox lock, so a pilot that
/// sends to the same test actor only appends to the pending list.
pub(crate) struct TestActor {
  mailbox: Mailbox,
  queue: Mutex<VecDeque<ReceivedMessage>>,
  ignore: Mutex<Option<Ignore>>,
}

impl TestActor {
  pub(crate) fn new() -> Self {
    Self { mailbox: Mailbox::new(), queue: Mutex::new(VecDeque::new()), ignore: Mutex::new(None) }
  }

  /// Runs the auto pilot on `message`, then queues it unless it is ignored.
  ///
  /// A message that arrives while another thread is handling one is left in the
  /// pending list, so the handling thread pilots it too.
  pub(crate) fn receive(&self, sender: &ActorRef, message: Message) {
    if !self.mailbox.offer(sender.clone(), message) {
      return;
    }
    self.enter();
  }

  /// Handles every pending message until none is left.
  ///
  /// The guard lowers the handling flag if a pilot panics; on the normal path
  /// `next_or_release` already lowered it, so `finish` keeps `Drop` from doing
  /// it again.
  fn enter(&self) {
    let guard = MailboxGuard::new(&self.mailbox);
    while let Some((sender, message)) = self.mailbox.next_or_release() {
      self.handle_one(&sender, message);
    }
    guard.finish();
  }

  fn handle_one(&self, sender: &ActorRef, message: Message) {
    let mut pilot = self.mailbox.take_pilot();
    let Some(active) = pilot.as_mut() else {
      self.enqueue(sender, message);
      return;
    };
    let outcome = active.run(sender, message.clone());
    self.enqueue(sender, message);
    if outcome == AutoPilotControl::KeepRunning {
      self.mailbox.restore_pilot(pilot);
    }
  }

  /// Installs `pilot`, which runs for the next message.
  pub(crate) fn set_auto_pilot(&self, pilot: Box<dyn AutoPilot>) {
    self.mailbox.restore_pilot(Some(pilot));
  }

  /// Installs the ignore filter.
  pub(crate) fn set_ignore(&self, filter: Ignore) {
    let mut slot = self.ignore.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = Some(filter);
  }

  /// Takes the oldest queued message.
  pub(crate) fn take_next(&self) -> Option<ReceivedMessage> {
    self.lock_queue().pop_front()
  }

  fn enqueue(&self, sender: &ActorRef, message: Message) {
    let ignored = {
      let ignore = self.ignore.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
      ignore.as_ref().is_some_and(|filter| filter(&message))
    };
    if ignored {
      return;
    }
    self.lock_queue().push_back(ReceivedMessage::new(message, sender.clone()));
  }

  fn lock_queue(&self) -> MutexGuard<'_, VecDeque<ReceivedMessage>> {
    self.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

/// The pending messages of the test actor and whether some thread is already
/// handling one.
///
/// `offer` decides "is somebody handling messages?" and appends to the pending
/// list in one lock, and `next_or_release` takes the next message or lowers the
/// flag in one lock. A message therefore cannot slip in between the decision to
/// stop handling and the decision to keep going: either the offering thread
/// raises the flag and handles the message itself, or the handling thread sees
/// it in the list before it lowers the flag.
struct Mailbox {
  state: Mutex<MailboxState>,
  pilot: Mutex<Option<Box<dyn AutoPilot>>>,
}

impl Mailbox {
  fn new() -> Self {
    Self { state: Mutex::new(MailboxState { pending: VecDeque::new(), handling: false }), pilot: Mutex::new(None) }
  }

  /// Appends a message and claims the right to handle it when no other thread is
  /// handling one. Returns whether the caller has to handle the messages.
  fn offer(&self, sender: ActorRef, message: Message) -> bool {
    let mut state = self.lock();
    state.pending.push_back((sender, message));
    if state.handling {
      return false;
    }
    state.handling = true;
    true
  }

  /// Takes the oldest pending message, or lowers the handling flag when the list
  /// is empty.
  fn next_or_release(&self) -> Option<(ActorRef, Message)> {
    let mut state = self.lock();
    match state.pending.pop_front() {
      | Some(next) => Some(next),
      | None => {
        state.handling = false;
        None
      },
    }
  }

  /// Takes the installed pilot, so it can run outside the mailbox lock.
  fn take_pilot(&self) -> Option<Box<dyn AutoPilot>> {
    self.lock_pilot().take()
  }

  /// Puts `pilot` back unless another thread already installed one.
  fn restore_pilot(&self, pilot: Option<Box<dyn AutoPilot>>) {
    let mut slot = self.lock_pilot();
    if slot.is_none() {
      *slot = pilot;
    }
  }

  fn lock_pilot(&self) -> MutexGuard<'_, Option<Box<dyn AutoPilot>>> {
    self.pilot.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  fn lock(&self) -> MutexGuard<'_, MailboxState> {
    self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

struct MailboxState {
  pending: VecDeque<(ActorRef, Message)>,
  handling: bool,
}

/// Lowers the handling flag of its mailbox if the handling thread unwinds, so a
/// panicking pilot does not stop the test actor for good.
///
/// `finish` must be called on the normal path; it makes the `Drop` a no-op, so
/// the flag that `next_or_release` already lowered is not lowered a second time
/// behind another thread's back.
struct MailboxGuard<'a> {
  mailbox: Option<&'a Mailbox>,
}

impl<'a> MailboxGuard<'a> {
  fn new(mailbox: &'a Mailbox) -> Self {
    Self { mailbox: Some(mailbox) }
  }

  fn finish(mut self) {
    self.mailbox = None;
  }
}

impl Drop for MailboxGuard<'_> {
  fn drop(&mut self) {
    if let Some(mailbox) = self.mailbox {
      mailbox.lock().handling = false;
    }
  }
}
