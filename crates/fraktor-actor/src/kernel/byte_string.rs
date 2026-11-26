// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

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
