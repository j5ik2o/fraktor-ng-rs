//! An immutable index that matches element sequences against wildcard
//! patterns.

use alloc::{collections::BTreeMap, string::String};

use super::WildcardIndexError;

#[cfg(test)]
#[path = "wildcard_index_value_test.rs"]
mod tests;

/// An immutable index from wildcard patterns to values.
///
/// Mirrors `org.apache.pekko.util.WildcardIndex` (marked `@InternalApi` in
/// Pekko). A pattern is a sequence of elements; an element is an exact name,
/// `"*"` (matches any single element), a name ending in `"*"` such as `"a*"`
/// (a suffix pattern, matching any element that starts with the part before
/// the `"*"`), or `"**"`. A pattern whose last element is `"**"` goes to the
/// double-wildcard tree and matches any number of remaining elements; a
/// `"**"` anywhere else is an ordinary child that never matches.
///
/// [`insert`](Self::insert) returns a new index and leaves `self` unchanged;
/// it copies the whole tree, so its cost is proportional to the size of the
/// tree, not to the length of the inserted pattern. Matching is greedy and
/// never backtracks, as in the original: at each element the exact child
/// wins, then the `"*"` child, then the first suffix child in key order whose
/// prefix the element starts with (the original iterates a hash map, so its
/// suffix order is unspecified; the port's is deterministic).
///
/// Two behaviours differ from the original on purpose; both are recorded as
/// intentional differences in the design document for #187, section 2.2.
/// First, a suffix child is looked up by the same stripped key it is stored
/// under, where the original looks it up by the unstripped element and loses
/// the earlier subtree when the same suffix pattern is inserted twice.
/// Second, the root emptiness check counts suffix children, where the
/// original's does not, so an index holding only a suffix pattern at the root
/// still matches; only the root check differs — the per-node emptiness that
/// decides the fallback to the double-wildcard tree is the original's.
#[derive(Debug, Clone)]
pub struct WildcardIndex<T> {
  wildcard_tree: WildcardTree<T>,
  double_wildcard_tree: WildcardTree<T>,
}

impl<T> Default for WildcardIndex<T> {
  fn default() -> Self {
    Self { wildcard_tree: WildcardTree::default(), double_wildcard_tree: WildcardTree::default() }
  }
}

#[derive(Debug, Clone)]
struct WildcardTree<T> {
  data: Option<T>,
  children: BTreeMap<String, WildcardTree<T>>,
  wildcard_suffix_children: BTreeMap<String, WildcardTree<T>>,
}

impl<T> Default for WildcardTree<T> {
  fn default() -> Self {
    Self { data: None, children: BTreeMap::new(), wildcard_suffix_children: BTreeMap::new() }
  }
}

impl<T> WildcardTree<T> {
  /// The original's per-node emptiness (line 59 of the original): suffix
  /// children do not count.
  fn is_structurally_empty(&self) -> bool {
    self.data.is_none() && self.children.is_empty()
  }

  /// The root emptiness of the port: suffix children count here, and only
  /// here (intentional difference 2).
  fn is_root_empty(&self) -> bool {
    self.data.is_none() && self.children.is_empty() && self.wildcard_suffix_children.is_empty()
  }
}

impl<T: Clone> WildcardIndex<T> {
  /// Creates an empty index.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns whether both trees are empty at the root.
  ///
  /// The root check counts suffix children (intentional difference 2); the
  /// original's `isEmpty` does not, so it reports an index holding only a
  /// suffix pattern as empty.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.wildcard_tree.is_root_empty() && self.double_wildcard_tree.is_root_empty()
  }

  /// Returns a new index that also maps `elems` to `value`.
  ///
  /// `self` is unchanged. A pattern whose last element is `"**"` goes to the
  /// double-wildcard tree; any other pattern goes to the single-wildcard
  /// tree. An empty `elems` returns a copy of `self`, as in the original,
  /// whose recursive insert has no node to attach the value to.
  ///
  /// # Errors
  ///
  /// Returns [`WildcardIndexError::DoubleWildcardAsSuffix`] when any element
  /// other than `"**"` itself ends with `"**"`. Every element is checked
  /// before anything is copied, so a failed insert changes nothing.
  pub fn insert(&self, elems: &[&str], value: T) -> Result<Self, WildcardIndexError> {
    for elem in elems {
      if *elem != "**" && elem.ends_with("**") {
        return Err(WildcardIndexError::DoubleWildcardAsSuffix { element: (*elem).into() });
      }
    }
    let mut next = self.clone();
    if elems.is_empty() {
      return Ok(next);
    }
    let tree = if elems.last() == Some(&"**") { &mut next.double_wildcard_tree } else { &mut next.wildcard_tree };
    let mut node = tree;
    for elem in elems {
      node = if *elem != "*" && *elem != "**" && elem.ends_with('*') {
        // The lookup key is the stripped key (intentional difference 1).
        let key = elem.strip_suffix('*').unwrap_or(elem);
        node.wildcard_suffix_children.entry(key.into()).or_default()
      } else {
        node.children.entry((*elem).into()).or_default()
      };
    }
    node.data = Some(value);
    Ok(next)
  }

  /// Returns the value whose pattern matches `elems`, if any.
  ///
  /// When the single-wildcard tree is empty at the root, or the node the
  /// single-wildcard walk reaches is structurally empty, the
  /// double-wildcard tree answers instead, as in the original.
  #[must_use]
  pub fn find(&self, elems: &[&str]) -> Option<&T> {
    if self.wildcard_tree.is_root_empty() {
      if self.double_wildcard_tree.is_root_empty() {
        return None;
      }
      return self.double_wildcard_tree.find_with_terminal_double_wildcard(elems).and_then(|tree| tree.data.as_ref());
    }
    match self.wildcard_tree.find_with_single_wildcard(elems) {
      | Some(tree) if !tree.is_structurally_empty() => tree.data.as_ref(),
      | _ => self.double_wildcard_tree.find_with_terminal_double_wildcard(elems).and_then(|tree| tree.data.as_ref()),
    }
  }
}

impl<T> WildcardTree<T> {
  /// The greedy single-wildcard walk. Returns the reached node, or `None`
  /// when no branch matches an element (the original's empty tree).
  fn find_with_single_wildcard(&self, elems: &[&str]) -> Option<&Self> {
    let mut node = self;
    for elem in elems {
      if let Some(child) = node.children.get(*elem) {
        node = child;
      } else if let Some(child) = node.children.get("*") {
        node = child;
      } else if let Some((_, child)) =
        node.wildcard_suffix_children.iter().find(|(prefix, _)| elem.starts_with(prefix.as_str()))
      {
        node = child;
      } else {
        return None;
      }
    }
    Some(node)
  }

  /// The terminal double-wildcard walk. The alternative is the `"**"` child
  /// of the deepest node seen so far; when no branch matches an element the
  /// alternative answers, and when the elements run out the reached node
  /// answers and the alternative is discarded, as in the original. Suffix
  /// children are never consulted here, also as in the original.
  fn find_with_terminal_double_wildcard(&self, elems: &[&str]) -> Option<&Self> {
    let mut node = self;
    let mut alternative: Option<&Self> = None;
    for elem in elems {
      alternative = node.children.get("**").or(alternative);
      if let Some(child) = node.children.get(*elem) {
        node = child;
      } else if let Some(child) = node.children.get("*") {
        node = child;
      } else {
        return alternative;
      }
    }
    Some(node)
  }
}
