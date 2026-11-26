// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2021-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright 2015 Ben Manes. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

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
