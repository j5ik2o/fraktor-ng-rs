// The property test for the FSM transition listeners of issue #194
// (docs/design/issue-194-specs.md section 6, property 1): random
// sequences of subscribe, unsubscribe, listener stop, tick, and stay
// are driven against the real `Fsm`, and a plain reference model in
// this file predicts the notices a probe receives. Quint checks the
// model; this test checks the Rust machine against the model.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::kernel::actor::{
  Actor, ActorContext, CurrentState, Fsm, FsmState, FsmView, Props, SubscribeTransitionCallBack, Transition,
  UnsubscribeTransitionCallBack,
};
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

struct SplitMix64(u64);

impl SplitMix64 {
  fn next(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }
}

struct Forwarder {
  target: Arc<Mutex<Vec<Message>>>,
}

impl Actor for Forwarder {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    self.target.lock().unwrap().push(message);
  }
}

fn machine() -> Fsm<i32, i32> {
  let mut fsm = Fsm::new();
  fsm.start_with(0, 0);
  fsm.when(0, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &i32| {
    match *event.downcast_ref::<&'static str>()? {
      | "tick" => Some(FsmState::goto(1)),
      | _ => Some(FsmState::stay()),
    }
  });
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &i32| {
    match *event.downcast_ref::<&'static str>()? {
      | "tick" => Some(FsmState::goto(0)),
      | _ => Some(FsmState::stay()),
    }
  });
  fsm.initialize();
  fsm
}

#[derive(Clone, Copy, Debug)]
enum Step {
  Subscribe(usize),
  Unsubscribe(usize),
  StopListener(usize),
  Tick,
  Stay,
}

#[test]
fn fsm_listeners_match_the_reference_model() {
  for seed in [1u64, 7, 42, 1_000, 99_991] {
    let mut spec = FraktorSpec::new("FsmTransitionProperty");
    let mut probe = TestProbe::new(spec.system_mut());
    let probe_ref = probe.ref_handle();
    let fsm = spec.system_mut().actor_of(Props::new(machine));
    let sender = spec.system().dead_letters();

    // Three listeners: the probe itself and two forwarders whose
    // inboxes the test reads directly.
    let inbox_a: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
    let inbox_b: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
    let box_a = inbox_a.clone();
    let box_b = inbox_b.clone();
    let listener_a = spec.system_mut().actor_of(Props::new(move || Forwarder { target: box_a.clone() }));
    let listener_b = spec.system_mut().actor_of(Props::new(move || Forwarder { target: box_b.clone() }));
    let listeners = [probe_ref.clone(), listener_a.clone(), listener_b.clone()];

    let mut rng = SplitMix64(seed);
    let mut steps = Vec::new();
    for _ in 0..60 {
      let step = match rng.next() % 8 {
        | 0 => Step::Subscribe((rng.next() % 3) as usize),
        | 1 => Step::Unsubscribe((rng.next() % 3) as usize),
        | 2 => Step::StopListener((rng.next() % 3) as usize),
        | 3..=5 => Step::Tick,
        | _ => Step::Stay,
      };
      steps.push(step);
    }

    // Reference model: the listener set, who is alive, the state, and
    // the notices each listener should have received, in order.
    let mut subscribed: BTreeSet<usize> = BTreeSet::new();
    let mut alive: [bool; 3] = [true; 3];
    let mut state = 0i32;
    let mut expected: [Vec<String>; 3] = [Vec::new(), Vec::new(), Vec::new()];

    for step in &steps {
      match *step {
        | Step::Subscribe(index) => {
          if alive[index] {
            fsm.tell(SubscribeTransitionCallBack(listeners[index].clone()), sender.clone());
            subscribed.insert(index);
            // Pekko answers every subscribe message with CurrentState.
            expected[index].push(format!("current:{state}"));
          }
        },
        | Step::Unsubscribe(index) => {
          fsm.tell(UnsubscribeTransitionCallBack(listeners[index].clone()), sender.clone());
          subscribed.remove(&index);
        },
        | Step::StopListener(index) => {
          if alive[index] {
            alive[index] = false;
            spec.system_mut().stop(&listeners[index]);
          }
        },
        | Step::Tick => {
          let from = state;
          state = 1 - state;
          fsm.tell("tick", sender.clone());
          for index in subscribed.iter() {
            if alive[*index] {
              expected[*index].push(format!("transition:{from}->{state}"));
            }
          }
          // A stopped listener is removed once its notice is
          // processed; the model drops it after this step's delivery
          // has run, which the drain below performs.
        },
        | Step::Stay => {
          fsm.tell("stay", sender.clone());
        },
      }
      spec.system_mut().run_until_idle();
      // After delivery, stopped listeners are out of the set.
      subscribed.retain(|index| alive[*index]);
    }

    // Compare: the probe's queue and the forwarders' inboxes.
    let mut probe_seen: Vec<String> = Vec::new();
    let drained = probe.receive_while(
      fraktor_testkit::test_kit::ReceiveWhile::new().idle(core::time::Duration::from_millis(50)),
      |message| {
        message.downcast_ref::<CurrentState<i32>>().map(|current| format!("current:{}", current.state)).or_else(|| {
          message
            .downcast_ref::<Transition<i32>>()
            .map(|transition| format!("transition:{}->{}", transition.from, transition.to))
        })
      },
    );
    probe_seen.extend(drained);
    assert_eq!(probe_seen, expected[0], "seed {seed}: probe notices differ; steps {steps:?}");
    for (index, inbox) in [inbox_a, inbox_b].iter().enumerate() {
      let seen: Vec<String> = inbox
        .lock()
        .unwrap()
        .iter()
        .map(|message| {
          if let Some(current) = message.downcast_ref::<CurrentState<i32>>() {
            format!("current:{}", current.state)
          } else if let Some(transition) = message.downcast_ref::<Transition<i32>>() {
            format!("transition:{}->{}", transition.from, transition.to)
          } else {
            "other".to_string()
          }
        })
        .collect();
      assert_eq!(seen, expected[index + 1], "seed {seed}: listener {} notices differ; steps {steps:?}", index + 1);
    }
    spec.terminate();
  }
}
