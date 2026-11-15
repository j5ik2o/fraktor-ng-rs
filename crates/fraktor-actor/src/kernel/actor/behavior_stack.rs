//! The stack of behaviours an actor installs with `become`.

use alloc::{boxed::Box, vec::Vec};

use crate::kernel::{actor::ActorContext, message::Message};

/// One behaviour frame that can answer a message.
///
/// It reads and writes the context of the delivery it handles, so it can
/// `become` or `unbecome` again while it runs.
pub(crate) type Frame = Box<dyn FnMut(&mut ActorContext<'_>, Message) + 'static>;

/// The head a delivery runs, taken out of the stack so it can be called.
///
/// Pekko runs `behaviorStack.head` without removing it, so `become` and
/// `unbecome` edit the stack while the running frame is still on it. The port
/// leaves a `None` placeholder in the head slot and hands the frame to the
/// caller; `finish_running` puts it back into that placeholder if the slot is
/// still there, which reproduces Pekko's stack for every `become`/`unbecome`
/// the frame issues.
pub(crate) enum Running {
  /// The stack was empty, so the initial `Actor::receive` runs and no slot was
  /// taken.
  Receive,
  /// The frame taken from `index` runs; its slot holds `None` until it returns.
  Frame { index: usize, frame: Frame },
}

/// The behaviours an actor installs above its initial `receive`.
///
/// Mirrors the `behaviorStack` of `org.apache.pekko.actor.ActorCell`: the head
/// is the last element, and `become`/`unbecome` edit the same list while a
/// frame runs. The port stores only the frames that `become` installed; the
/// initial `Actor::receive` runs when the stack is empty, which stands for
/// Pekko's implicit `receive` bottom frame.
///
/// While a delivery runs, the head slot holds `None` (see [`Running`]); the
/// `become`/`unbecome` operations below still read and write `frames` as the
/// head-containing stack Pekko holds, so the placeholder takes the place of the
/// running frame exactly.
pub(crate) struct BehaviorStack {
  frames: Vec<Option<Frame>>,
}

impl BehaviorStack {
  /// Creates an empty stack, which runs the initial behaviour.
  pub(crate) fn new() -> Self {
    Self { frames: Vec::new() }
  }

  /// Installs `frame` as the new head, dropping the previous head below it.
  ///
  /// Mirrors `become(frame, discardOld = true)`, which is
  /// `behavior :: behaviorStack.tail` when the stack is non-empty: only the
  /// head is dropped and the frames below it stay.
  pub(crate) fn change(&mut self, frame: Frame) {
    self.frames.pop();
    self.frames.push(Some(frame));
  }

  /// Installs `frame` above the current head.
  ///
  /// Mirrors `become(frame, discardOld = false)`, which is
  /// `behavior :: behaviorStack`.
  pub(crate) fn push(&mut self, frame: Frame) {
    self.frames.push(Some(frame));
  }

  /// Drops the head, restoring the frame below it.
  ///
  /// Mirrors `unbecome`, which drops the head and leaves the frame below it (or
  /// the initial `receive`, the implicit bottom frame, when the stack empties).
  pub(crate) fn pop(&mut self) {
    self.frames.pop();
  }

  /// Takes the head out of the stack so it can run.
  ///
  /// The head slot is left as `None` while the frame runs, so the
  /// `change`/`push`/`pop` it calls operate on the same list Pekko holds. An
  /// empty stack runs the initial `receive`.
  pub(crate) fn take_running(&mut self) -> Running {
    match self.frames.last_mut() {
      | Some(slot @ Some(_)) => {
        let frame = slot.take().expect("the head slot held a frame");
        Running::Frame { index: self.frames.len() - 1, frame }
      },
      | _ => Running::Receive,
    }
  }

  /// Puts the running frame back into its head slot when it is still the slot.
  ///
  /// If the frame replaced the head with `become` or dropped it with
  /// `unbecome`, the slot is gone or holds another frame, so the running frame
  /// does not return, as Pekko has it.
  pub(crate) fn finish_running(&mut self, running: Running) {
    if let Running::Frame { index, frame } = running {
      if let Some(slot) = self.frames.get_mut(index) {
        if slot.is_none() {
          *slot = Some(frame);
        }
      }
    }
  }
}
