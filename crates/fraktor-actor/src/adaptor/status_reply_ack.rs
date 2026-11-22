//! The acknowledgement a request actor sends when it has nothing to return.

/// The acknowledgement payload of a successful [`StatusReply::ack`](super::StatusReply::ack).
///
/// Corresponds to Pekko's `Done`. This is the acknowledgement value, distinct
/// from the [`StatusReply`](super::StatusReply) envelope that carries success
/// or failure. `StatusReply::ack` wraps this value as its success payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusReplyAck;
