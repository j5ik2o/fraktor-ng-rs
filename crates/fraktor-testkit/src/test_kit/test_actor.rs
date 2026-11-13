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
pub(crate) struct TestActor {
  queue: Mutex<VecDeque<ReceivedMessage>>,
  ignore: Mutex<Option<Ignore>>,
  auto_pilot: Mutex<Option<Box<dyn AutoPilot>>>,
}

impl TestActor {
  pub(crate) fn new() -> Self {
    Self { queue: Mutex::new(VecDeque::new()), ignore: Mutex::new(None), auto_pilot: Mutex::new(None) }
  }

  /// Runs the auto pilot on `message`, then queues it unless it is ignored.
  ///
  /// The pilot is taken out of the lock while it runs, so a pilot that sends
  /// to the same test actor does not deadlock against its own queue.
  pub(crate) fn receive(&self, sender: &ActorRef, message: Message) {
    let pilot = self.auto_pilot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
    let mut pilot = pilot;
    let still_running = {
      let Some(active) = pilot.as_mut() else {
        self.enqueue(sender, message);
        return;
      };
      let outcome = active.run(sender, message.clone());
      self.enqueue(sender, message);
      outcome == AutoPilotControl::KeepRunning
    };
    if still_running {
      let mut slot = self.auto_pilot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
      if slot.is_none() {
        *slot = pilot;
      }
    }
  }

  /// Installs `pilot`, which runs for the next message.
  pub(crate) fn set_auto_pilot(&self, pilot: Box<dyn AutoPilot>) {
    let mut slot = self.auto_pilot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = Some(pilot);
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
