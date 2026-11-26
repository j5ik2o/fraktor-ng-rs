// Model-based property tests for WildcardIndex.
//
// The model is a second, declarative implementation of the matching rules in
// the design document for #187 (W1 to W6): it keeps the inserted routes in a
// list and, for a query, narrows the set of candidate routes segment by
// segment, because the greedy branch choice depends on which branches the
// other routes have. The candidate set keeps normal children and suffix
// children as distinct branch kinds even when their names coincide. The two
// intentional differences from the original (the suffix-child lookup key, and
// the root emptiness check) are the fixed forms. 400 fixed-seed operation
// sequences of 40 operations are compared through the public API.

use fraktor_actor::kernel::wildcard_index::WildcardIndex;

struct SplitMix64(u64);

impl SplitMix64 {
  fn next_u64(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next_u64() % bound
  }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Branch {
  Exact,
  Star,
  Suffix,
  Double,
}

#[derive(Clone)]
struct Route {
  branches: Vec<(Branch, String)>,
  value:    i32,
  double:   bool,
}

fn classify(elem: &str) -> (Branch, String) {
  if elem == "*" {
    (Branch::Star, String::new())
  } else if elem == "**" {
    (Branch::Double, String::new())
  } else if let Some(prefix) = elem.strip_suffix('*') {
    (Branch::Suffix, prefix.to_string())
  } else {
    (Branch::Exact, elem.to_string())
  }
}

fn model_insert(routes: &mut Vec<Route>, elems: &[&str], value: i32) {
  // Double routes keep their terminal "**" as a final Double branch: in the
  // tree the value sits on the "**" child node itself.
  let double = elems.last() == Some(&"**");
  let branches: Vec<(Branch, String)> = elems.iter().map(|elem| classify(elem)).collect();
  let route = Route { branches, value, double };
  routes.retain(|existing| !(existing.branches == route.branches && existing.double == route.double));
  routes.push(route);
}

/// Whether any route passes through a node with this prefix in the given tree.
fn node_exists(routes: &[Route], prefix: &[(Branch, String)], double: bool) -> bool {
  routes.iter().any(|route| route.double == double && route.branches.starts_with(prefix))
}

/// The structural emptiness of W2: a node is structurally empty when no route
/// ends exactly at it and it has no normal (exact, star or double) children.
/// Suffix children do not count.
fn structurally_empty(routes: &[Route], prefix: &[(Branch, String)], double: bool) -> bool {
  let ends_here = routes.iter().any(|route| route.double == double && route.branches == prefix);
  let has_normal_child = routes.iter().any(|route| {
    route.double == double
      && route.branches.len() > prefix.len()
      && route.branches.starts_with(prefix)
      && route.branches[prefix.len()].0 != Branch::Suffix
  });
  !ends_here && !has_normal_child
}

/// The root emptiness of W4: suffix children count at the root only.
fn root_empty(routes: &[Route], double: bool) -> bool {
  !routes.iter().any(|route| route.double == double)
}

fn value_at(routes: &[Route], prefix: &[(Branch, String)], double: bool) -> Option<i32> {
  routes.iter().find(|route| route.double == double && route.branches == prefix).map(|route| route.value)
}

/// Greedy single-wildcard walk (W1). Returns the reached prefix, or None when
/// the walk fell off the tree (the original's empty tree).
fn walk_single(routes: &[Route], elems: &[&str]) -> Option<Vec<(Branch, String)>> {
  let mut prefix: Vec<(Branch, String)> = Vec::new();
  for elem in elems {
    let exact = (Branch::Exact, (*elem).to_string());
    let star = (Branch::Star, String::new());
    if node_exists(routes, &with(&prefix, &exact), false) {
      prefix.push(exact);
    } else if node_exists(routes, &with(&prefix, &star), false) {
      prefix.push(star);
    } else {
      // Suffix children: the smallest matching key wins, as in the port.
      let mut candidates: Vec<&(Branch, String)> = routes
        .iter()
        .filter(|route| !route.double && route.branches.len() > prefix.len() && route.branches.starts_with(&prefix))
        .map(|route| &route.branches[prefix.len()])
        .filter(|branch| branch.0 == Branch::Suffix && elem.starts_with(branch.1.as_str()))
        .collect();
      candidates.sort_by(|a, b| a.1.cmp(&b.1));
      candidates.dedup();
      match candidates.first() {
        | Some(branch) => prefix.push((*branch).clone()),
        | None => return None,
      }
    }
  }
  Some(prefix)
}

/// Terminal double-wildcard walk (W3). Suffix children are never consulted.
fn walk_double(routes: &[Route], elems: &[&str]) -> Option<Vec<(Branch, String)>> {
  fn go(
    routes: &[Route],
    elems: &[&str],
    prefix: &[(Branch, String)],
    alternative: Option<Vec<(Branch, String)>>,
  ) -> Option<Vec<(Branch, String)>> {
    let Some((elem, rest)) = elems.split_first() else {
      return Some(prefix.to_vec());
    };
    let double_child = (Branch::Double, String::new());
    let new_alternative =
      if node_exists(routes, &with(prefix, &double_child), true) { Some(with(prefix, &double_child)) } else { alternative };
    let exact = (Branch::Exact, (*elem).to_string());
    let star = (Branch::Star, String::new());
    if node_exists(routes, &with(prefix, &exact), true) {
      go(routes, rest, &with(prefix, &exact), new_alternative)
    } else if node_exists(routes, &with(prefix, &star), true) {
      go(routes, rest, &with(prefix, &star), new_alternative)
    } else {
      new_alternative
    }
  }
  go(routes, elems, &[], None)
}

fn with(prefix: &[(Branch, String)], branch: &(Branch, String)) -> Vec<(Branch, String)> {
  let mut out = prefix.to_vec();
  out.push(branch.clone());
  out
}

fn model_find(routes: &[Route], elems: &[&str]) -> Option<i32> {
  if root_empty(routes, false) {
    if root_empty(routes, true) {
      return None;
    }
    return walk_double(routes, elems).and_then(|prefix| value_at(routes, &prefix, true));
  }
  match walk_single(routes, elems) {
    | Some(prefix) if !structurally_empty(routes, &prefix, false) => value_at(routes, &prefix, false),
    | _ => walk_double(routes, elems).and_then(|prefix| value_at(routes, &prefix, true)),
  }
}

fn model_is_empty(routes: &[Route]) -> bool {
  root_empty(routes, false) && root_empty(routes, true)
}

const SEGMENTS: [&str; 8] = ["a", "b", "x", "*", "a*", "b*", "**", "a**"];

fn random_route(random: &mut SplitMix64) -> Vec<&'static str> {
  let len = random.below(4) as usize;
  (0..len).map(|_| SEGMENTS[random.below(SEGMENTS.len() as u64) as usize]).collect()
}

#[test]
fn wildcard_index_matches_the_model_over_random_operation_sequences() {
  for seed in 0..400u64 {
    let mut index: WildcardIndex<i32> = WildcardIndex::new();
    let mut routes: Vec<Route> = Vec::new();
    let mut random = SplitMix64(0x1872_0000 + seed);
    assert!(index.is_empty());
    for step in 0..40 {
      if random.below(3) == 0 {
        let query = random_route(&mut random);
        assert_eq!(index.find(&query).copied(), model_find(&routes, &query), "seed {seed} step {step} query {query:?}");
      } else {
        let route = random_route(&mut random);
        let value = (seed * 1000 + step) as i32;
        let invalid = route.iter().any(|elem| *elem != "**" && elem.ends_with("**"));
        match index.insert(&route, value) {
          | Ok(next) => {
            assert!(!invalid, "seed {seed} step {step} route {route:?} must be rejected");
            index = next;
            model_insert(&mut routes, &route, value);
          },
          | Err(_) => {
            assert!(invalid, "seed {seed} step {step} route {route:?} must be accepted");
          },
        }
        assert_eq!(index.is_empty(), model_is_empty(&routes));
      }
    }
  }
}
