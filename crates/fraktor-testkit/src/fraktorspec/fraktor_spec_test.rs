//! Unit tests for [`FraktorSpec`](super::FraktorSpec).

use std::sync::Arc;
use std::sync::Mutex;

use super::FraktorSpec;
use crate::fraktorspec::FraktorSpecHooks;
use crate::test_kit::TestKit;

struct RecordingHooks {
  events: Arc<Mutex<Vec<&'static str>>>,
}

impl FraktorSpecHooks for RecordingHooks {
  fn at_startup(&mut self, _test_kit: &mut TestKit) {
    self.events.lock().unwrap().push("at_startup");
  }

  fn before_termination(&mut self, test_kit: &mut TestKit) {
    assert!(!test_kit.system().is_terminated(), "before_termination runs while the system is running");
    self.events.lock().unwrap().push("before_termination");
  }

  fn after_termination(&mut self, test_kit: &mut TestKit) {
    assert!(test_kit.system().is_terminated(), "after_termination runs after the system stopped");
    self.events.lock().unwrap().push("after_termination");
  }
}

#[test]
fn with_hooks_runs_at_startup_once() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let hooks = RecordingHooks { events: Arc::clone(&events) };

  let _spec = FraktorSpec::with_hooks("PekkoSpecSpec", Box::new(hooks));

  assert_eq!(*events.lock().unwrap(), vec!["at_startup"]);
}

#[test]
fn terminate_runs_the_hooks_in_order() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let hooks = RecordingHooks { events: Arc::clone(&events) };
  let mut spec = FraktorSpec::with_hooks("PekkoSpecSpec", Box::new(hooks));

  spec.terminate();

  assert_eq!(*events.lock().unwrap(), vec!["at_startup", "before_termination", "after_termination"]);
}

#[test]
fn a_second_terminate_does_nothing() {
  let events = Arc::new(Mutex::new(Vec::new()));
  let hooks = RecordingHooks { events: Arc::clone(&events) };
  let mut spec = FraktorSpec::with_hooks("PekkoSpecSpec", Box::new(hooks));

  spec.terminate();
  spec.terminate();

  assert_eq!(events.lock().unwrap().len(), 3);
}

#[test]
fn new_creates_a_running_system() {
  let spec = FraktorSpec::new("PekkoSpecSpec");

  assert!(!spec.system().is_terminated());
  assert_eq!(spec.system().name(), "PekkoSpecSpec");
}
