//! Count-min sketches that estimate how often values were seen, with the
//! TinyLFU reset that periodically halves every counter.

mod bits;
mod counter_bits;
mod counter_matrix;
mod fast_frequency_sketch;
mod fast_frequency_sketch_settings;
mod frequency_sketch_error;
mod frequency_sketch_hasher;
mod frequency_sketch_settings;
mod frequency_sketch_value;
mod hasher_seal;
mod murmur3;
mod reset_size;
mod sketch_depth;
mod sketch_width;
mod string_hash_code;
mod string_hasher;

pub use fast_frequency_sketch::FastFrequencySketch;
pub use fast_frequency_sketch_settings::FastFrequencySketchSettings;
pub use frequency_sketch_error::FrequencySketchError;
pub use frequency_sketch_hasher::FrequencySketchHasher;
pub use frequency_sketch_settings::FrequencySketchSettings;
pub use frequency_sketch_value::FrequencySketch;
pub use string_hash_code::StringHashCode;
pub use string_hasher::StringHasher;
