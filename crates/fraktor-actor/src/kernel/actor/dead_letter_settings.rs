//! The settings of the dead letter listener.

use core::num::NonZeroU32;
use core::time::Duration;

/// How many dead letters the listener logs before it stops or suspends.
///
/// Mirrors the `pekko.log-dead-letters` setting: `off` logs nothing, a
/// count logs that many, and `unlimited` logs every one. Pekko reads the
/// setting as an `Int`, where 0 means off and `Int.MaxValue` means
/// unlimited; the port names the three cases, and a count of zero cannot
/// be written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeadLetterLogLimit {
  /// Log no dead letters. The listener still counts nothing and logs
  /// nothing.
  Off,
  /// Log at most this many dead letters in one logging period.
  Count(NonZeroU32),
  /// Log every dead letter.
  Unlimited,
}

/// The values the dead letter listener runs with.
///
/// Mirrors the two settings Pekko's `DeadLetterListener` reads:
/// `pekko.log-dead-letters` and `pekko.log-dead-letters-suspend-duration`.
/// The port has no configuration layer for them, so the values are handed
/// to
/// [`ActorSystem::start_dead_letter_listener`](crate::kernel::actor_system::ActorSystem::start_dead_letter_listener)
/// directly. Pekko's default suspend duration is 5 minutes; the default
/// here states it.
#[derive(Clone, Copy)]
pub struct DeadLetterSettings {
  limit: DeadLetterLogLimit,
  suspend_duration: Option<Duration>,
}

impl DeadLetterSettings {
  /// Creates the settings with a log `limit` and an optional suspend
  /// duration.
  ///
  /// Without a suspend duration the listener stops itself when it reaches
  /// the limit, as Pekko does when the setting is not a finite duration.
  pub fn new(limit: DeadLetterLogLimit, suspend_duration: Option<Duration>) -> Self {
    Self { limit, suspend_duration }
  }

  /// Returns the settings Pekko's defaults describe: a limit of 10 and a
  /// suspend duration of 5 minutes.
  pub fn defaults() -> Self {
    Self {
      limit: DeadLetterLogLimit::Count(NonZeroU32::new(10).expect("10 is not zero")),
      suspend_duration: Some(Duration::from_secs(5 * 60)),
    }
  }

  /// Returns the log limit.
  pub fn limit(&self) -> DeadLetterLogLimit {
    self.limit
  }

  /// Returns the suspend duration, when one is set.
  pub fn suspend_duration(&self) -> Option<Duration> {
    self.suspend_duration
  }
}
