//! How many dead letters the listener logs before it stops or suspends.

use core::num::NonZeroU32;

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
