// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/HotSwapSpec.scala
//
// Original test names are kept as comments next to each ported case.
//
// The original builds each actor from an anonymous subclass of `Becomer` (an
// abstract `Actor`) or of `Actor`, and swaps its behavior with
// `context.become` / `context.unbecome`. The port models the behavior stack on
// the production `ActorContext`. `become` is a reserved word in Rust, so the
// methods are named for what they do: `change_behavior` mirrors
// `become(behavior, discardOld = true)` and replaces the current behavior,
// `push_behavior` mirrors `become(behavior, discardOld = false)` and stacks a
// frame that a later `pop_behavior` drops, and `pop_behavior` mirrors
// `unbecome`. The initial `Actor::receive` runs below every frame. The executor
// runs the top frame for each message, so a case swaps behavior through the
// context exactly as the original does.
//
// The original lets `become` run inside the constructor. The constructor body
// of the original is the recipe that `Props` holds, and the port runs that
// setup in `Actor::pre_start`, which the executor calls with the context before
// the first message. A `change_behavior` there is therefore active before any
// message arrives, which is the observable contract of "become in the
// constructor".
//
// Every case calls the production `ActorContext::change_behavior`,
// `push_behavior`, or `pop_behavior`. Until those exist on the production type
// the file does not compile: that compile error is the intended red for this
// node.

use fraktor_actor::kernel::{
  actor::{Actor, ActorContext, Props},
  actor_ref::ActorRef,
  actor_system::ActorSystem,
  message::Message,
};
use fraktor_testkit::{event_filter::EventFilter, fraktorspec::FraktorSpec, implicit_sender::ImplicitSender};

const SPEC_SYSTEM_NAME: &str = "HotSwapSpec";

const CRASH_MESSAGE: &str = "Crash (expected)!";

fn text_of(message: &Message) -> &'static str {
  message.downcast_ref::<&'static str>().copied().unwrap_or("FAILURE")
}

// Original: `Props(new Becomer { context.become { case always => sender() !
// always }; def receive = { case _ => sender() ! "FAILURE" } })`
//
// The constructor calls `become` once, so the frame answers the first message
// with the text it received, and `receive` never runs.
struct BecomerOneFrame;

impl Actor for BecomerOneFrame {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.change_behavior(|context: &mut ActorContext<'_>, message: Message| {
      let reply = String::from(text_of(&message));
      context.sender().tell(reply, context.self_ref());
    });
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    context.sender().tell(String::from("FAILURE"), context.self_ref());
  }
}

// Original: `for (i <- 1 to 4) context.become { case always => sender() ! s"$i:$always" }`
//
// Each `become` replaces the previous frame (discardOld = true), so only the
// last one answers: the message arrives as `4:<text>`.
struct BecomerFourFramesDiscarding;

impl Actor for BecomerFourFramesDiscarding {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    for i in 1..=4_u32 {
      context.change_behavior(move |context: &mut ActorContext<'_>, message: Message| {
        let reply = format!("{i}:{}", text_of(&message));
        context.sender().tell(reply, context.self_ref());
      });
    }
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    context.sender().tell(String::from("FAILURE"), context.self_ref());
  }
}

// Original: `context.become({ case always => sender() ! "pigdog:" + always;
// context.unbecome() }, false); def receive = { case always => sender() !
// "badass:" + always }`
//
// One frame stacks above the initial behavior. The first message goes to the
// frame, which answers and then pops itself, so the next message reaches
// `receive`.
struct BecomerStackedOnce;

impl Actor for BecomerStackedOnce {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.push_behavior(|context: &mut ActorContext<'_>, message: Message| {
      let reply = format!("pigdog:{}", text_of(&message));
      context.sender().tell(reply, context.self_ref());
      context.pop_behavior();
    });
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let reply = format!("badass:{}", text_of(&message));
    context.sender().tell(reply, context.self_ref());
  }
}

// Original: `for (i <- 1 to 4) context.become({ case always => sender() ! s"$i:$always";
// context.unbecome() }, false)`
//
// Four frames stack above the initial behavior, so four messages each answer
// with their own index and pop one frame: 4, 3, 2, 1.
struct BecomerFourFramesStacked;

impl Actor for BecomerFourFramesStacked {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    for i in 1..=4_u32 {
      context.push_behavior(move |context: &mut ActorContext<'_>, message: Message| {
        let reply = format!("{i}:{}", text_of(&message));
        context.sender().tell(reply, context.self_ref());
        context.pop_behavior();
      });
    }
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    context.sender().tell(String::from("FAILURE"), context.self_ref());
  }
}

// Original: a plain `Actor` whose `receive` answers "init", and on "swap"
// becomes a frame that echoes a `String` back to `context.sender()`.
struct SwapToEcho;

impl Actor for SwapToEcho {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "init" {
      context.sender().tell(String::from("init"), context.self_ref());
    } else if message == "swap" {
      context.change_behavior(|context: &mut ActorContext<'_>, message: Message| {
        if let Some(text) = message.downcast_ref::<&'static str>() {
          context.sender().tell((*text).to_string(), context.self_ref());
        }
      });
    }
  }
}

// Original: a plain `Actor` that answers "init", and on "swap" becomes a frame
// that answers "swapped" and returns to the initial behavior on "revert".
struct SwapAndRevert;

impl Actor for SwapAndRevert {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "init" {
      context.sender().tell(String::from("init"), context.self_ref());
    } else if message == "swap" {
      context.change_behavior(|context: &mut ActorContext<'_>, message: Message| {
        if message == "swapped" {
          context.sender().tell(String::from("swapped"), context.self_ref());
        } else if message == "revert" {
          context.pop_behavior();
        }
      });
    }
  }
}

// Original: a plain `Actor` whose initial behavior answers "state" with "0",
// and on "swap" becomes a frame that answers "state" with "1", "swapped" with
// "swapped", and throws `new Exception("Crash (expected)!")` on "crash".
//
// A restart re-creates the instance from the recipe, so the frame that "swap"
// pushed is gone after the crash and the initial behavior answers again.
struct SwapThenCrash;

impl Actor for SwapThenCrash {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "state" {
      context.sender().tell(String::from("0"), context.self_ref());
    } else if message == "swap" {
      context.change_behavior(|context: &mut ActorContext<'_>, message: Message| {
        if message == "state" {
          context.sender().tell(String::from("1"), context.self_ref());
        } else if message == "swapped" {
          context.sender().tell(String::from("swapped"), context.self_ref());
        } else if message == "crash" {
          panic!("{CRASH_MESSAGE}");
        }
      });
      context.sender().tell(String::from("swapped"), context.self_ref());
    }
  }
}

// The original HotSwapSpec turns each message into a behavior; it does not
// swap behavior from inside a behavior frame that is already running. These
// cases exercise that path so `become`/`push`/`unbecome` called from the
// running frame leave the same stack as Pekko's `ActorCell`. Each case sets its
// stack up in `pre_start`, then a message makes the running frame swap.

// The top frame pushes `Inner` from inside the frame that is running. Pekko's
// `become(Inner, discardOld = false)` puts `Inner` above the running frame, so
// the next message reaches `Inner`, not the running frame again.
struct PushFromRunningFrame;

impl Actor for PushFromRunningFrame {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.push_behavior(|context: &mut ActorContext<'_>, message: Message| {
      if message == "go" {
        context.push_behavior(|context: &mut ActorContext<'_>, message: Message| {
          if message == "go" {
            context.sender().tell(String::from("inner"), context.self_ref());
          }
        });
        context.sender().tell(String::from("outer"), context.self_ref());
      }
    });
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    context.sender().tell(String::from("receive"), context.self_ref());
  }
}

// The top frame replaces itself with a new frame from inside the frame that is
// running. Pekko's `become(new, discardOld = true)` replaces the running head
// and keeps the frame below, so the next messages reach the new frame and then
// the frame below.
struct ChangeFromRunningFrame;

impl Actor for ChangeFromRunningFrame {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.push_behavior(|context: &mut ActorContext<'_>, message: Message| {
      if message == "below" {
        context.sender().tell(String::from("below"), context.self_ref());
      }
    });
    context.push_behavior(|context: &mut ActorContext<'_>, message: Message| {
      if message == "swap" {
        context.change_behavior(|context: &mut ActorContext<'_>, message: Message| {
          if message == "top" {
            context.sender().tell(String::from("top"), context.self_ref());
            context.pop_behavior();
          }
        });
        context.sender().tell(String::from("swapped"), context.self_ref());
      }
    });
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    context.sender().tell(String::from("receive"), context.self_ref());
  }
}

// The top frame pops itself while it runs. Pekko drops the running frame, so
// the next message reaches the frame below.
struct PopFromRunningFrame;

impl Actor for PopFromRunningFrame {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.push_behavior(|context: &mut ActorContext<'_>, message: Message| {
      if message == "below" {
        context.sender().tell(String::from("below"), context.self_ref());
      }
    });
    context.push_behavior(|context: &mut ActorContext<'_>, message: Message| {
      if message == "pop" {
        context.sender().tell(String::from("top"), context.self_ref());
        context.pop_behavior();
      }
    });
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    context.sender().tell(String::from("receive"), context.self_ref());
  }
}

// The top frame performs two structural operations in one delivery: it pushes
// `Inner` and then pops it again. Pekko's `[F]` -> `become(Inner, false)` ->
// `[Inner, F]` -> `unbecome` -> `[F]` leaves the running frame on top, so the
// next message reaches the same frame again; a port that returns the running
// frame below a stale head, or drops it, answers differently.
struct MultiOpFromRunningFrame;

impl Actor for MultiOpFromRunningFrame {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.push_behavior(|context: &mut ActorContext<'_>, message: Message| {
      if message == "go" {
        context.push_behavior(|context: &mut ActorContext<'_>, _message: Message| {
          context.sender().tell(String::from("inner"), context.self_ref());
        });
        context.pop_behavior();
        context.sender().tell(String::from("outer"), context.self_ref());
      }
    });
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    context.sender().tell(String::from("receive"), context.self_ref());
  }
}

// The top frame answers every message and never swaps. Pekko runs the head
// without removing it, so the same frame keeps answering; the port must restore
// the frame into its head slot after each delivery for the next one to reach it.
struct SteadyFrame;

impl Actor for SteadyFrame {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.push_behavior(|context: &mut ActorContext<'_>, message: Message| {
      let reply = format!("steady:{}", text_of(&message));
      context.sender().tell(reply, context.self_ref());
    });
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let reply = format!("receive:{}", text_of(&message));
    context.sender().tell(reply, context.self_ref());
  }
}

struct HotSwapSpecCase {
  fraktor_spec: FraktorSpec,
}

impl HotSwapSpecCase {
  fn new() -> Self {
    Self { fraktor_spec: FraktorSpec::new(SPEC_SYSTEM_NAME) }
  }

  fn system(&mut self) -> &mut ActorSystem {
    self.fraktor_spec.system_mut()
  }

  fn self_actor_ref(&self) -> ActorRef {
    ImplicitSender::self_actor_ref(&self.fraktor_spec)
  }

  fn actor_of(&mut self, props: Props) -> ActorRef {
    self.system().actor_of(props)
  }

  fn tell(&mut self, target: &ActorRef, message: &'static str) {
    let sender = self.self_actor_ref();
    target.tell(message, sender);
  }

  fn expect_str(&mut self, expected: &str) {
    let found: String = self.fraktor_spec.expect_msg_type();
    assert_eq!(found, expected, "expected {expected:?}, found {found:?}");
  }

  // Original: `EventFilter[Exception](message = "Crash (expected)!", occurrences = 1).intercept { a
  // ! "crash" }`
  //
  // Pekko's `new Exception("Crash (expected)!")` matched by `EventFilter[Exception]`
  // because the cause is an `Exception`. The port maps a throw inside `receive`
  // to the `RuntimeException` cause, whose `Exception` parent the original
  // filter also accepts; the port filter compares the cause name exactly, so it
  // names that cause. The message is the one the original throws.
  fn intercept_crash(&mut self, target: &ActorRef) {
    let filter = EventFilter::typed("RuntimeException").with_message(CRASH_MESSAGE);
    let sender = self.self_actor_ref();
    let system = self.fraktor_spec.system_mut();
    let target = target.clone();
    EventFilter::intercept(&filter, system, move || {
      target.tell("crash", sender);
    });
  }
}

impl Drop for HotSwapSpecCase {
  fn drop(&mut self) {
    self.fraktor_spec.terminate();
  }
}

// "An Actor" must { "be able to become in its constructor" in { ... } }
#[test]
fn be_able_to_become_in_its_constructor() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| BecomerOneFrame));

  case.tell(&actor, "pigdog");
  case.expect_str("pigdog");
}

// "An Actor" must { "be able to become multiple times in its constructor" in { ... } }
#[test]
fn be_able_to_become_multiple_times_in_its_constructor() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| BecomerFourFramesDiscarding));

  case.tell(&actor, "pigdog");
  case.expect_str("4:pigdog");
}

// "An Actor" must { "be able to become with stacking in its constructor" in { ... } }
#[test]
fn be_able_to_become_with_stacking_in_its_constructor() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| BecomerStackedOnce));

  case.tell(&actor, "pigdog");
  case.expect_str("pigdog:pigdog");
  case.tell(&actor, "badass");
  case.expect_str("badass:badass");
}

// "An Actor" must { "be able to become, with stacking, multiple times in its constructor" in { ...
// } }
#[test]
fn be_able_to_become_with_stacking_multiple_times_in_its_constructor() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| BecomerFourFramesStacked));

  case.tell(&actor, "pigdog");
  case.tell(&actor, "pigdog");
  case.tell(&actor, "pigdog");
  case.tell(&actor, "pigdog");
  case.expect_str("4:pigdog");
  case.expect_str("3:pigdog");
  case.expect_str("2:pigdog");
  case.expect_str("1:pigdog");
}

// "An Actor" must { "be able to hotswap its behavior with become(..)" in { ... } }
#[test]
fn be_able_to_hotswap_its_behavior_with_become() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| SwapToEcho));

  case.tell(&actor, "init");
  case.expect_str("init");
  case.tell(&actor, "swap");
  case.tell(&actor, "swapped");
  case.expect_str("swapped");
}

// "An Actor" must { "be able to revert hotswap its behavior with unbecome" in { ... } }
#[test]
fn be_able_to_revert_hotswap_its_behavior_with_unbecome() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| SwapAndRevert));

  case.tell(&actor, "init");
  case.expect_str("init");
  case.tell(&actor, "swap");

  case.tell(&actor, "swapped");
  case.expect_str("swapped");

  case.tell(&actor, "revert");
  case.tell(&actor, "init");
  case.expect_str("init");
}

// "An Actor" must { "revert to initial state on restart" in { ... } }
#[test]
fn revert_to_initial_state_on_restart() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| SwapThenCrash));

  case.tell(&actor, "state");
  case.expect_str("0");
  case.tell(&actor, "swap");
  case.expect_str("swapped");
  case.tell(&actor, "state");
  case.expect_str("1");
  case.intercept_crash(&actor);
  case.tell(&actor, "state");
  case.expect_str("0");
}

// Additional case (not in the original HotSwapSpec): a frame calls
// `push_behavior` while it runs. Pekko puts the pushed frame above the running
// frame, so the next message reaches the pushed frame.
#[test]
fn push_behavior_from_running_frame_puts_the_new_frame_on_top() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| PushFromRunningFrame));

  case.tell(&actor, "go");
  case.expect_str("outer");
  case.tell(&actor, "go");
  case.expect_str("inner");
}

// Additional case (not in the original HotSwapSpec): a frame calls
// `change_behavior` (discardOld = true) while it runs. Pekko replaces the
// running head with the new frame and keeps the frame below, so the next
// messages reach the new frame and then the frame below.
#[test]
fn change_behavior_from_running_frame_replaces_head_and_keeps_tail() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| ChangeFromRunningFrame));

  case.tell(&actor, "swap");
  case.expect_str("swapped");
  case.tell(&actor, "top");
  case.expect_str("top");
  case.tell(&actor, "below");
  case.expect_str("below");
}

// Additional case (not in the original HotSwapSpec): a frame calls
// `pop_behavior` while it runs. Pekko drops the running frame, so the next
// message reaches the frame below.
#[test]
fn pop_behavior_from_running_frame_reaches_the_frame_below() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| PopFromRunningFrame));

  case.tell(&actor, "pop");
  case.expect_str("top");
  case.tell(&actor, "below");
  case.expect_str("below");
}

// Additional case (not in the original HotSwapSpec): a frame that never swaps
// answers two consecutive messages. Pekko runs `behaviorStack.head` without
// removing it, so the same frame answers again; this fixes the port's restore
// of the running frame into its head slot after a delivery (`finish_running`).
#[test]
fn a_frame_that_does_not_swap_answers_consecutive_messages() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| SteadyFrame));

  case.tell(&actor, "one");
  case.expect_str("steady:one");
  case.tell(&actor, "two");
  case.expect_str("steady:two");
}

// Additional case (not in the original HotSwapSpec): a frame does two
// structural operations in one delivery (`push_behavior` then `pop_behavior`).
// Pekko ends with the same frame on top (`[F]` -> `[Inner, F]` -> `[F]`), so the
// next message reaches it again. This is the multi-operation sequence that a
// `take_running`/restore port can get wrong.
#[test]
fn a_running_frame_can_push_and_pop_within_one_delivery() {
  let mut case = HotSwapSpecCase::new();
  let actor = case.actor_of(Props::new(|| MultiOpFromRunningFrame));

  case.tell(&actor, "go");
  case.expect_str("outer");
  case.tell(&actor, "go");
  case.expect_str("outer");
}
