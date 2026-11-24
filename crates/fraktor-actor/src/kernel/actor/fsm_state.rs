//! The next state a finite state machine moves to after an event.

/// What a state function of an [`Fsm`](crate::kernel::actor::Fsm) answers for
/// one event: the state to move to, whether the move is reported to the
/// transition handlers, and the data to keep.
///
/// Mirrors `org.apache.pekko.actor.FSM.State` as `goto`, `stay`, and `using`
/// build it. Pekko's `goto` and `stay` are methods of the machine that copy the
/// current data; the port builds the value without the machine and lets the
/// machine keep its current data unless `using` replaced it.
pub struct FsmState<S, D> {
  target: Option<S>,
  data: Option<D>,
}

impl<S, D> FsmState<S, D> {
  /// Moves to `state`, which the transition handlers hear about even when it
  /// is the current state.
  ///
  /// Mirrors `FSM.goto(nextStateName)`, whose state notifies the handlers.
  pub fn goto(state: S) -> Self {
    Self { target: Some(state), data: None }
  }

  /// Keeps the current state without telling the transition handlers.
  ///
  /// Mirrors `FSM.stay()`, which is `goto(stateName)` with the notification
  /// switched off.
  pub fn stay() -> Self {
    Self { target: None, data: None }
  }

  /// Replaces the data the machine keeps in the next state.
  ///
  /// Mirrors `FSM.State.using(nextStateData)`.
  pub fn using(mut self, data: D) -> Self {
    self.data = Some(data);
    self
  }

  pub(crate) fn into_parts(self) -> (Option<S>, Option<D>) {
    (self.target, self.data)
  }
}
