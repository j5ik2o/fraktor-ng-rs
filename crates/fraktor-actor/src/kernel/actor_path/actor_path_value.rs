//! A path identifying an actor below the root of an actor system.

use core::fmt;

use crate::kernel::actor_path::root_actor_path::RootActorPath;
use crate::kernel::address::Address;
use crate::kernel::address::UriParts;

/// The special symbols that may appear inside an actor path element.
const VALID_SYMBOLS: &str = "-_.*$+:@&=,!~';";

/// A concrete actor path made of a root address and its child element names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActorPath {
  root: RootActorPath,
  elements: Vec<String>,
}

impl ActorPath {
  pub(crate) fn new(root: RootActorPath, elements: Vec<String>) -> Self {
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
      .map(|element| element.to_string())
      .collect();
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
  /// Panics when `child` contains the path separator `/` or the fragment
  /// separator `#`.
  pub fn with_child(&self, child: &str) -> Self {
    assert!(!child.contains('/'), "/ is a path separator and is not legal in ActorPath names: [{}]", child);
    assert!(!child.contains('#'), "# is a fragment separator and is not legal in ActorPath names: [{}]", child);
    let mut elements = self.elements.clone();
    elements.push(child.to_string());
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
      self.elements.clone()
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
    format!("/{}", self.elements.join("/"))
  }

  /// Returns this path, replacing the address with `address` unless the
  /// path already carries a host.
  pub fn to_string_with_address(&self, address: &Address) -> String {
    if self.root.address().host().is_some() {
      self.to_string()
    } else {
      format!("{}{}{}", address, self.root.name(), self.elements.join("/"))
    }
  }
}

impl fmt::Display for ActorPath {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}{}{}", self.root.address(), self.root.name(), self.elements.join("/"))
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
