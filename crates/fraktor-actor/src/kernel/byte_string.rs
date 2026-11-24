//! An immutable sequence of bytes and a stream that reads it.

mod byte_string_input_stream;
mod byte_string_value;

pub use byte_string_input_stream::ByteStringInputStream;
pub use byte_string_value::ByteString;
