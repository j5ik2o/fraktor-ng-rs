//! The result of parsing a `resolv.conf`.

use alloc::{string::String, vec::Vec};

#[cfg(test)]
#[path = "resolv_conf_test.rs"]
mod tests;

/// The search list and `ndots` a `resolv.conf` holds.
///
/// Mirrors `org.apache.pekko.io.dns.internal.ResolvConf`, the `case class` that
/// `ResolvConfParser.parseLines` builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvConf {
  search: Vec<String>,
  // A plain `i32`, as the original's `Int`: the original puts no range or
  // format rule on it beyond being an integer (a negative value is kept), and
  // nothing in this port computes with it yet. A dedicated type belongs with
  // the first logic that interprets the threshold.
  ndots: i32,
}

impl ResolvConf {
  /// Creates a configuration from its search list and `ndots`.
  pub fn new(search: Vec<String>, ndots: i32) -> Self {
    Self { search, ndots }
  }

  /// Returns the domains to search, in order.
  pub fn search(&self) -> &[String] {
    &self.search
  }

  /// Returns the number of dots a name needs before it is tried as absolute.
  pub fn ndots(&self) -> i32 {
    self.ndots
  }
}
