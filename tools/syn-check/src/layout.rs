/// Which repository layout the path-based scope predicates assume.
///
/// Rules that depend on where a file lives (kernel or adaptor code, library
/// examples) keep one predicate per layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
  /// fraktor-ng-rs: `crates/<crate>/src/kernel/**` and `crates/<crate>/src/adaptor/**`.
  Ng,
  /// The old fraktor-rs: `modules/<name>-core*/` and `modules/<name>-adaptor-std/`.
  /// The legacy UI fixtures and the old corpus are checked with this layout.
  Legacy,
}

impl Layout {
  pub fn parse(name: &str) -> Option<Self> {
    match name {
      | "ng" => Some(Self::Ng),
      | "legacy" => Some(Self::Legacy),
      | _ => None,
    }
  }

  pub fn name(self) -> &'static str {
    match self {
      | Self::Ng => "ng",
      | Self::Legacy => "legacy",
    }
  }
}
