//! An immutable sequence of bytes, a builder for it, and a stream that reads it.

mod byte_string_builder;
mod byte_string_input_stream;
mod byte_string_value;
mod end_of_stream_error;
mod index_out_of_bounds_error;
mod mark_not_supported_error;

pub use byte_string_builder::ByteStringBuilder;
pub use byte_string_input_stream::ByteStringInputStream;
pub use byte_string_value::ByteString;
pub use end_of_stream_error::EndOfStreamError;
pub use index_out_of_bounds_error::IndexOutOfBoundsError;
pub use mark_not_supported_error::MarkNotSupportedError;
