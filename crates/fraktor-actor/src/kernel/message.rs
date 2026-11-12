//! Messages and envelopes that travel between actors.

mod envelope;
mod message_value;

pub use envelope::Envelope;
pub use message_value::Message;
