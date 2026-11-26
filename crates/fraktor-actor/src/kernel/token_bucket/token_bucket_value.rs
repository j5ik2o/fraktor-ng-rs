// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2015-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A token bucket over a monotonic clock.

use crate::kernel::Clock;

use super::TokenBucketError;

#[cfg(test)]
#[path = "token_bucket_value_test.rs"]
mod tests;

/// A token bucket that tells the caller how long an element must be delayed
/// to conform with the bucket's capacity and rate.
///
/// Mirrors `org.apache.pekko.util.TokenBucket`, the internal rate limiter
/// whose `currentTime` is abstract in the original. This port reads the time
/// through the kernel's [`Clock`] port instead: the current time is
/// `clock.monotonic().as_nanos() as i64`, the low 64 bits of the nanosecond
/// count, so only differences between two readings carry meaning, and they
/// are taken with wrapping arithmetic exactly as the original takes them
/// with `Long`. A bucket over `adaptor::SystemClock` is the counterpart of
/// the original's `NanoTimeTokenBucket`.
///
/// [`init`](Self::init) must be called before the bucket can be used. If an
/// offer returns a non-zero delay, it is the caller's responsibility not to
/// call [`offer`](Self::offer) again before that delay has elapsed; the
/// bucket does not check or protect against early calls.
pub struct TokenBucket<C> {
  capacity: i64,
  nanos_between_tokens: i64,
  available_tokens: i64,
  last_update: i64,
  clock: C,
}

impl<C: Clock> TokenBucket<C> {
  /// Creates a bucket that holds at most `capacity` tokens and gains one
  /// token every `nanos_between_tokens` nanoseconds, reading the time from
  /// `clock`.
  ///
  /// # Errors
  ///
  /// Returns [`TokenBucketError::NegativeCapacity`] when `capacity` is
  /// negative, and [`TokenBucketError::NonPositivePeriod`] when
  /// `nanos_between_tokens` is zero or negative, in that order.
  pub fn new(capacity: i64, nanos_between_tokens: i64, clock: C) -> Result<Self, TokenBucketError> {
    if capacity < 0 {
      return Err(TokenBucketError::NegativeCapacity { capacity });
    }
    if nanos_between_tokens <= 0 {
      return Err(TokenBucketError::NonPositivePeriod { nanos_between_tokens });
    }
    Ok(Self { capacity, nanos_between_tokens, available_tokens: 0, last_update: 0, clock })
  }

  /// Fills the bucket and anchors the token arrival grid at the current
  /// time. Must be called before the bucket can be used.
  pub fn init(&mut self) {
    self.available_tokens = self.capacity;
    self.last_update = self.current_time();
  }

  /// Returns the capacity of the bucket.
  pub fn capacity(&self) -> i64 {
    self.capacity
  }

  /// Returns the time between two tokens, in nanoseconds.
  pub fn nanos_between_tokens(&self) -> i64 {
    self.nanos_between_tokens
  }

  /// Offers an element that costs `cost` tokens, and returns the number of
  /// nanoseconds the element needs to be delayed, or zero when it can be
  /// emitted immediately. The cost can be larger than the capacity.
  ///
  /// The method does not handle overflow: if an element is to be delayed
  /// longer than what can be represented as a positive `i64`, a wrapped
  /// value is returned, as in the original.
  ///
  /// # Errors
  ///
  /// Returns [`TokenBucketError::NegativeCost`] when `cost` is negative.
  /// The bucket's state is not changed in that case.
  pub fn offer(&mut self, cost: i64) -> Result<i64, TokenBucketError> {
    if cost < 0 {
      return Err(TokenBucketError::NegativeCost { cost });
    }

    let now = self.current_time();
    let time_elapsed = now.wrapping_sub(self.last_update);

    let tokens_arrived = if time_elapsed >= self.nanos_between_tokens {
      if time_elapsed < self.nanos_between_tokens.wrapping_mul(2) {
        // Only one tick elapsed.
        self.last_update = self.last_update.wrapping_add(self.nanos_between_tokens);
        1
      } else {
        let tokens_arrived = time_elapsed / self.nanos_between_tokens;
        self.last_update = self.last_update.wrapping_add(tokens_arrived.wrapping_mul(self.nanos_between_tokens));
        tokens_arrived
      }
    } else {
      0
    };

    // The saturating add is the one place the port differs from the
    // original's wrapping `math.min(availableTokens + tokensArrived,
    // capacity)`: when the sum does not fit in an `i64`, the original's
    // available tokens go negative (or far above the capacity), while the
    // port keeps the invariant that the available tokens stay within
    // `0..=capacity` for every input.
    self.available_tokens = self.available_tokens.saturating_add(tokens_arrived).min(self.capacity);

    if cost <= self.available_tokens {
      self.available_tokens -= cost;
      Ok(0)
    } else {
      let remaining_cost = cost - self.available_tokens;
      // Tokens always arrive at exact multiples of the token generation
      // period; the time since the last arrival must be accounted for, so
      // that the arrival grid does not shift.
      let time_since_token_arrival = now.wrapping_sub(self.last_update);
      let delay = remaining_cost.wrapping_mul(self.nanos_between_tokens).wrapping_sub(time_since_token_arrival);
      self.available_tokens = 0;
      self.last_update = now.wrapping_add(delay);
      Ok(delay)
    }
  }

  fn current_time(&self) -> i64 {
    self.clock.monotonic().as_nanos() as i64
  }
}
