// Property tests for the dead letter routing of issue #191.
//
// The cases of `DeadLetterSupressionSpec` check two fixed sequences. These
// tests drive random sequences of sends through a real system and compare
// the published events against a small model of the routing rules in the
// design (`docs/design/issue-191-specs.md`, sections 2.2 and 3): a message
// to a stopped actor or to the dead letters handle publishes exactly one
// event; a suppressed payload publishes a `SuppressedDeadLetter` naming the
// dead letters handle; a `DeadLetter` that arrives at the dead letters
// handle is published as it is, unless its own message is suppressed, and a
// `DeadLetter` addressed to a stopped actor is dropped, so nothing loops.
//
// The generator is a SplitMix64 counter, as in
// `frequency_sketch_property_test.rs`: no new dependency, and a failing
// seed is printed so the sequence can be replayed.

use core::time::Duration;
use std::string::String;
use std::sync::Arc;
use std::sync::Mutex;
use std::vec::Vec;

use fraktor_actor::kernel::actor::AnyDeadLetter;
use fraktor_actor::kernel::actor::DeadLetter;
use fraktor_actor::kernel::actor::DeadLetterLogLimit;
use fraktor_actor::kernel::actor::DeadLetterSettings;
use fraktor_actor::kernel::actor::DeadLetterSuppression;
use fraktor_actor::kernel::actor::SuppressedDeadLetter;
use fraktor_actor::kernel::actor::Terminated;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::DeadLetterListener;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_actors::echo_actor_props;

#[derive(Clone, Debug, PartialEq, Eq)]
struct NormalMsg(i32);

#[derive(Clone, Debug, PartialEq, Eq)]
struct SuppressedMsg;

impl DeadLetterSuppression for SuppressedMsg {}

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn new(seed: u64) -> Self {
    Self { state: seed }
  }

  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next_u64() % bound
  }
}

/// One step the generator picks: a destination and what is sent to it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
  NormalToStopped,
  SuppressedToStopped,
  NormalToDeadLetters,
  SuppressedToDeadLetters,
  DeadLetterToStopped,
  SuppressedDeadLetterToDeadLetters,
  NormalDeadLetterToDeadLetters,
}

/// The summary of one published event, in the order events were published.
fn describe(event: &AnyDeadLetter<'_>) -> String {
  let (kind, payload) = match event {
    | AnyDeadLetter::DeadLetter(d) => ("dead", payload_tag(d.message())),
    | AnyDeadLetter::Suppressed(d) => ("suppressed", payload_tag(d.message())),
    | AnyDeadLetter::Dropped(d) => ("dropped", payload_tag(d.message())),
    | AnyDeadLetter::Unhandled(u) => ("unhandled", payload_tag(u.message())),
  };
  format!("{kind}:{payload}:to={}", event.recipient().path())
}

fn payload_tag(message: &Message) -> String {
  if message.downcast_ref::<NormalMsg>().is_some() {
    return String::from("normal");
  }
  if message.downcast_ref::<SuppressedMsg>().is_some() {
    return String::from("suppressed-msg");
  }
  if message.downcast_ref::<DeadLetter>().is_some() {
    return String::from("dead-letter");
  }
  String::from("other")
}

/// What the model expects one step to publish, as `describe` summaries.
fn model_step(step: Step, stopped: &ActorRef, dead_letters: &ActorRef, live: &ActorRef) -> Vec<String> {
  let stopped_path = stopped.path().to_string();
  let dead_path = dead_letters.path().to_string();
  let live_path = live.path().to_string();
  match step {
    | Step::NormalToStopped => vec![format!("dead:normal:to={stopped_path}")],
    | Step::SuppressedToStopped => vec![format!("suppressed:suppressed-msg:to={dead_path}")],
    | Step::NormalToDeadLetters => vec![format!("dead:normal:to={dead_path}")],
    | Step::SuppressedToDeadLetters => vec![format!("suppressed:suppressed-msg:to={dead_path}")],
    // A DeadLetter addressed to a stopped actor is dropped (rule D7).
    | Step::DeadLetterToStopped => Vec::new(),
    // A DeadLetter whose message is suppressed becomes a SuppressedDeadLetter.
    | Step::SuppressedDeadLetterToDeadLetters => vec![format!("suppressed:suppressed-msg:to={dead_path}")],
    // A DeadLetter that reaches the dead letters handle is published as it is.
    | Step::NormalDeadLetterToDeadLetters => vec![format!("dead:normal:to={live_path}")],
  }
}

#[test]
fn routing_matches_the_model_for_random_sequences() {
  for seed in 0..24_u64 {
    let mut spec = FraktorSpec::new("DeadLetterRoutingProperty");
    spec.system_mut().register_dead_letter_suppression::<SuppressedMsg>();
    let sender = spec.test_actor();
    let dead_letters = spec.system().dead_letters();

    let live = spec.system_mut().actor_of(echo_actor_props());
    let stopped = spec.system_mut().actor_of(echo_actor_props());
    spec.test_kit_mut().watch(&stopped);
    stopped.tell("PoisonPill", sender.clone());
    let _: Terminated = spec.expect_msg_type();

    let collected = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = collected.clone();
    let collector = spec.system_mut().function_ref("collector", move |_: &ActorRef, message: Message| {
      if let Some(event) = AnyDeadLetter::from_message(&message) {
        sink.lock().expect("the collector lock is not poisoned").push(describe(&event));
      }
    });
    let stream = spec.system().event_stream();
    stream.subscribe(Some(collector.clone()), EventClass::of::<DeadLetter>()).expect("the collector is not null");
    stream
      .subscribe(Some(collector.clone()), EventClass::of::<SuppressedDeadLetter>())
      .expect("the collector is not null");
    // The Terminated of the stopped actor reaches the test actor only; the
    // collector is subscribed after the stop, so it sees no leftovers.

    let mut rng = SplitMix64::new(seed.wrapping_mul(0x1000) + 7);
    let mut expected: Vec<String> = Vec::new();
    for index in 0..40 {
      let step = match rng.below(7) {
        | 0 => Step::NormalToStopped,
        | 1 => Step::SuppressedToStopped,
        | 2 => Step::NormalToDeadLetters,
        | 3 => Step::SuppressedToDeadLetters,
        | 4 => Step::DeadLetterToStopped,
        | 5 => Step::SuppressedDeadLetterToDeadLetters,
        | _ => Step::NormalDeadLetterToDeadLetters,
      };
      expected.extend(model_step(step, &stopped, &dead_letters, &live));
      let payload = NormalMsg(index);
      match step {
        | Step::NormalToStopped => stopped.tell(payload, sender.clone()),
        | Step::SuppressedToStopped => stopped.tell(SuppressedMsg, sender.clone()),
        | Step::NormalToDeadLetters => dead_letters.tell(payload, sender.clone()),
        | Step::SuppressedToDeadLetters => dead_letters.tell(SuppressedMsg, sender.clone()),
        | Step::DeadLetterToStopped => {
          stopped.tell(DeadLetter::new(payload, sender.clone(), live.clone()), sender.clone())
        },
        | Step::SuppressedDeadLetterToDeadLetters => {
          dead_letters.tell(DeadLetter::new(SuppressedMsg, sender.clone(), live.clone()), sender.clone())
        },
        | Step::NormalDeadLetterToDeadLetters => {
          dead_letters.tell(DeadLetter::new(payload, sender.clone(), live.clone()), sender.clone())
        },
      }
      // Drive the shard after each step: a tell to the dead letters handle
      // publishes at once, while a tell to a stopped actor is routed when
      // the executor delivers it, so the published order matches the step
      // order only when the shard is drained between steps.
      spec.system_mut().run_until_idle();
    }
    let actual = collected.lock().expect("the collector lock is not poisoned").clone();
    assert_eq!(actual, expected, "seed {seed}: the published events differ from the model");
    spec.terminate();
  }
}

/// The listener core against a simple model: random event kinds and random
/// clock advances; the model recomputes the count, the suspension, and the
/// log texts from the rules of the design (section 2.3).
#[test]
fn listener_core_matches_the_model_for_random_sequences() {
  let is_suppressed = |message: &Message| message.downcast_ref::<SuppressedMsg>().is_some();
  for seed in 0..24_u64 {
    let mut rng = SplitMix64::new(seed.wrapping_mul(0x2000) + 13);
    let limit = 1 + rng.below(5) as u32;
    let limit = DeadLetterLogLimit::Count(core::num::NonZeroU32::new(limit).expect("the limit is not zero"));
    let settings = DeadLetterSettings::new(limit, Some(Duration::from_secs(2)));
    // The dead letters handle of a scratch system stands in for the
    // listener's own handle; only its identity is compared.
    let mut spec = FraktorSpec::new("DeadLetterListenerProperty");
    let dead_letters = spec.system().dead_letters();
    let sender = spec.test_actor();
    let recipient = dead_letters.clone();
    let mut core = DeadLetterListener::new(settings, dead_letters.clone());

    // Model state.
    let mut count: u32 = 0;
    let mut suspended_until: Option<Duration> = None;
    let mut now = Duration::ZERO;
    for _ in 0..60 {
      match rng.below(6) {
        | 0 => now += Duration::from_millis(rng.below(3000)),
        | _ => {
          let kind = rng.below(4);
          let dl_normal = DeadLetter::new(NormalMsg(1), sender.clone(), recipient.clone());
          let dl_wrapped = DeadLetter::new(
            DeadLetter::new(SuppressedMsg, sender.clone(), recipient.clone()),
            sender.clone(),
            recipient.clone(),
          );
          let dropped =
            fraktor_actor::kernel::actor::Dropped::new(NormalMsg(1), "reason", sender.clone(), recipient.clone());
          let unhandled =
            fraktor_actor::kernel::actor::UnhandledMessage::new(NormalMsg(1), sender.clone(), recipient.clone());
          let event = match kind {
            | 0 => AnyDeadLetter::DeadLetter(&dl_normal),
            | 1 => AnyDeadLetter::DeadLetter(&dl_wrapped),
            | 2 => AnyDeadLetter::Dropped(&dropped),
            | _ => AnyDeadLetter::Unhandled(&unhandled),
          };
          let logged = core.on_event(&event, now, &is_suppressed);
          // Model: a DeadLetter wrapping a suppressed message is not counted.
          let wrapped_suppressed = matches!(&event, AnyDeadLetter::DeadLetter(d)
            if d.message().downcast_ref::<DeadLetter>()
              .is_some_and(|inner| inner.message().downcast_ref::<SuppressedMsg>().is_some()));
          if wrapped_suppressed {
            assert!(logged.is_none(), "seed {seed}: a wrapped suppressed message is not logged");
            continue;
          }
          count = if count == u32::MAX { 1 } else { count + 1 };
          match suspended_until {
            | Some(until) => {
              if now > until {
                let expected_tail = format!(", of which {} were not logged", count - limit_value(limit) - 1);
                let text = logged.expect("the resume logs one event");
                assert!(text.contains(&expected_tail), "seed {seed}: resume text {text:?}");
                assert!(text.contains(&format!("[{count}] dead letters encountered")), "seed {seed}: count in text");
                count = 0;
                suspended_until = None;
              } else {
                assert!(logged.is_none(), "seed {seed}: nothing is logged while suspended");
              }
            },
            | None => {
              let text = logged.expect("an event is logged while logging is on");
              assert!(text.contains(&format!("[{count}] dead letters encountered")), "seed {seed}: count in text");
              if count == limit_value(limit) {
                assert!(
                  text.contains(", no more dead letters will be logged in next [2.000 s]"),
                  "seed {seed}: {text:?}"
                );
                suspended_until = Some(now + Duration::from_secs(2));
              }
            },
          }
        },
      }
    }
    spec.terminate();
  }
}

fn limit_value(limit: DeadLetterLogLimit) -> u32 {
  match limit {
    | DeadLetterLogLimit::Count(n) => n.get(),
    | _ => u32::MAX,
  }
}
