use core::time::Duration;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

struct Passive;

impl Actor for Passive {
  fn receive(&mut self, _: &mut ActorContext<'_>, _: Message) {}
}

fn assert_unknown_watch_terminates(spec: &mut FraktorSpec, target: &ActorRef) {
  let mut probe = TestProbe::with_name(spec.system_mut(), "watcher");
  probe.watch(target);
  let termination = probe.expect_msg_terminated_within(Duration::from_millis(100));
  assert_eq!(termination.target(), target);
  assert!(!termination.existence_confirmed());
  assert!(!termination.address_terminated());
}

#[test]
fn an_actor_of_another_system_still_gets_an_immediate_termination_response() {
  let mut local = FraktorSpec::new("WatchLocal");
  let mut other = FraktorSpec::new("WatchOther");
  let target = other.system_mut().actor_of(Props::new(|| Passive));
  assert_unknown_watch_terminates(&mut local, &target);
}

#[test]
fn watching_an_already_stopped_function_reference_still_reports_termination() {
  let mut spec = FraktorSpec::new("WatchFunction");
  let target = spec.system_mut().function_ref("function", |_, _| {});
  spec.system_mut().stop(&target);
  assert_unknown_watch_terminates(&mut spec, &target);
}
