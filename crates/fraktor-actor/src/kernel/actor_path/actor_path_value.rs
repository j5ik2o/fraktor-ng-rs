//! A path identifying an actor below the root of an actor system.

use core::fmt;

use crate::kernel::actor_path::root_actor_path::RootActorPath;
use crate::kernel::address::Address;
use crate::kernel::address::UriParts;

#[cfg(test)]
#[path = "actor_path_value_test.rs"]
mod tests;

/// The special symbols that may appear inside an actor path element.
const VALID_SYMBOLS: &str = "-_.*$+:@&=,!~';";

/// A single element of an actor path, which must be non-empty and carry no
/// separator.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct PathElement(String);

impl PathElement {
  /// Creates a path element, or returns an error when `text` is empty or
  /// carries the separators `/` or `#`.
  pub(crate) fn new(text: &str) -> Result<Self, String> {
    if text.is_empty() {
      return Err("Actor path element must not be empty".to_string());
    }
    if text.contains('/') {
      return Err(format!("/ is a path separator and is not legal in ActorPath names: [{}]", text));
    }
    if text.contains('#') {
      return Err(format!("# is a fragment separator and is not legal in ActorPath names: [{}]", text));
    }
    Ok(Self(text.to_string()))
  }

  /// Parses a path element, returning `None` when it is not legal.
  pub(crate) fn parse(text: &str) -> Option<Self> {
    Self::new(text).ok()
  }

  /// Returns the element text.
  pub(crate) fn as_str(&self) -> &str {
    &self.0
  }
}

/// The name at the root of an actor path, which is `/` or a single leading
/// segment without a separator.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct RootName(String);

impl RootName {
  /// Creates a root name, or returns an error when it is empty, contains the
  /// path separator `/` after the first character, or carries the fragment
  /// separator `#`.
  ///
  /// A single-character name (including `/`) is accepted unchanged, so a root
  /// name created by this implementation keeps the accepted set of the earlier
  /// `with_name`.
  pub(crate) fn new(name: &str) -> Result<Self, String> {
    if !(name.len() == 1 || name.get(1..).is_some_and(|rest| !rest.contains('/'))) {
      return Err(format!(
        "/ may only exist at the beginning of the root actors name, \
                 it is a path separator and is not legal in ActorPath names: [{}]",
        name
      ));
    }
    if name.contains('#') {
      return Err(format!("# is a fragment separator and is not legal in ActorPath names: [{}]", name));
    }
    Ok(Self(name.to_string()))
  }

  /// Returns the root name, which is `/` for the root of a hierarchy.
  pub(crate) fn root() -> Self {
    Self("/".to_string())
  }

  /// Returns the name text.
  pub(crate) fn as_str(&self) -> &str {
    &self.0
  }
}

/// A concrete actor path made of a root address and its child element names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActorPath {
  root: RootActorPath,
  elements: Vec<PathElement>,
}

impl ActorPath {
  pub(crate) fn new(root: RootActorPath, elements: Vec<PathElement>) -> Self {
    Self { root, elements }
  }

  /// Returns the root path of an actor system, with no child elements.
  pub(crate) fn from_root(root: RootActorPath) -> Self {
    Self::new(root, Vec::new())
  }

  /// Parses an actor path, returning `None` when the string is malformed.
  pub fn from_string(s: &str) -> Option<Self> {
    let parts = UriParts::parse(s)?;
    let address = Address::from_uri_parts(&parts)?;
    let root = RootActorPath::new(address);
    let elements = parts
      .raw_path()
      .split('/')
      .skip(1)
      .filter(|element| !element.is_empty())
      .map(PathElement::parse)
      .collect::<Option<Vec<PathElement>>>()?;
    Some(Self::new(root, elements))
  }

  /// Validates a path element and returns an error message when it is illegal.
  pub fn validate_path_element(element: &str) -> Result<(), String> {
    match find_invalid_path_element_char_position(element) {
      | PathCode::Valid => Ok(()),
      | PathCode::Empty => Err("Actor path element must not be empty ".to_string()),
      | PathCode::Invalid(position, illegal) => Err(format!(
        "Invalid actor path element [{}], illegal character [{}] at position: {}. \
                     Actor paths MUST: not start with `$`, include only ASCII letters and can \
                     only contain these special characters: {}.",
        element, illegal, position, VALID_SYMBOLS
      )),
    }
  }

  /// Returns the child path with `child` appended.
  ///
  /// # Panics
  ///
  /// Panics when `child` is empty or contains the path separator `/` or the
  /// fragment separator `#`.
  pub fn with_child(&self, child: &str) -> Self {
    let element = PathElement::new(child).unwrap_or_else(|message| panic!("{message}"));
    let mut elements = self.elements.clone();
    elements.push(element);
    Self::new(self.root.clone(), elements)
  }

  /// Returns the root of this path.
  pub fn root(&self) -> &RootActorPath {
    &self.root
  }

  /// Returns the names from the root to this path.
  pub fn elements(&self) -> Vec<String> {
    if self.elements.is_empty() {
      vec![String::new()]
    } else {
      self.elements.iter().map(PathElement::as_str).map(str::to_string).collect()
    }
  }

  /// Returns the name of the last path element, or the root name when the
  /// path has no elements.
  ///
  /// Mirrors `org.apache.pekko.actor.ActorPath.name`, which a test compares
  /// with the name an actor was created under.
  pub fn name(&self) -> &str {
    match self.elements.last() {
      | Some(name) => name.as_str(),
      | None => self.root.name(),
    }
  }

  /// Returns the path without its last element, or this path when it has no
  /// element.
  ///
  /// Mirrors `org.apache.pekko.actor.ActorPath.parent`.
  pub fn parent(&self) -> Self {
    let mut elements = self.elements.clone();
    elements.pop();
    Self::new(self.root.clone(), elements)
  }

  /// Returns the path elements joined as `/a/b`, or `/` for the root.
  pub fn to_string_without_address(&self) -> String {
    format!("/{}", self.elements.iter().map(PathElement::as_str).collect::<Vec<_>>().join("/"))
  }

  /// Returns this path, replacing the address with `address` unless the
  /// path already carries a host.
  pub fn to_string_with_address(&self, address: &Address) -> String {
    if self.root.address().host().is_some() {
      self.to_string()
    } else {
      format!(
        "{}{}{}",
        address,
        self.root.name(),
        self.elements.iter().map(PathElement::as_str).collect::<Vec<_>>().join("/")
      )
    }
  }
}

impl fmt::Display for ActorPath {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      formatter,
      "{}{}{}",
      self.root.address(),
      self.root.name(),
      self.elements.iter().map(PathElement::as_str).collect::<Vec<_>>().join("/")
    )
  }
}

enum PathCode {
  Valid,
  Empty,
  Invalid(usize, char),
}

fn find_invalid_path_element_char_position(s: &str) -> PathCode {
  if s.is_empty() {
    return PathCode::Empty;
  }
  let chars: Vec<char> = s.chars().collect();
  if chars[0] == '$' {
    return PathCode::Invalid(0, '$');
  }
  let mut position = 0;
  while position < chars.len() {
    let c = chars[position];
    if is_valid_char(c) {
      position += 1;
    } else if c == '%'
      && position + 2 < chars.len()
      && is_hex_char(chars[position + 1])
      && is_hex_char(chars[position + 2])
    {
      position += 3;
    } else {
      return PathCode::Invalid(position, c);
    }
  }
  PathCode::Valid
}

fn is_valid_char(c: char) -> bool {
  c.is_ascii_alphanumeric() || VALID_SYMBOLS.contains(c)
}

fn is_hex_char(c: char) -> bool {
  c.is_ascii_hexdigit()
}
