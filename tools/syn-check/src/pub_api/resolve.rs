// SPDX-License-Identifier: Apache-2.0

//! Reachability walk that emits canonical public API items from a `ModuleGraph`.

use std::collections::BTreeSet;

use super::{ModuleGraph, PubItem, PubUse, UseKind};

pub(crate) fn emit_public_items(graph: &ModuleGraph, root_id: usize) -> Vec<String> {
  let mut out = BTreeSet::new();
  for name in &graph.macro_exports {
    out.insert(format!("pub macro {}::{name}!", graph.crate_name));
  }
  let mut visited = BTreeSet::new();
  let root_prefix = vec![graph.crate_name.clone()];
  walk_module(graph, root_id, &root_prefix, &mut visited, &mut out);
  out.into_iter().collect()
}

fn walk_module(
  graph: &ModuleGraph,
  mod_id: usize,
  prefix: &[String],
  visited: &mut BTreeSet<(usize, Vec<String>)>,
  out: &mut BTreeSet<String>,
) {
  if !visited.insert((mod_id, prefix.to_vec())) {
    return;
  }
  let module = &graph.modules[mod_id];

  for item in &module.pub_items {
    emit_item(graph, mod_id, prefix, &item.name, item, out);
  }

  for edge in &module.submodules {
    if !edge.is_pub {
      continue;
    }
    let mut child_prefix = prefix.to_vec();
    child_prefix.push(edge.name.clone());
    out.insert(format!("pub mod {}", child_prefix.join("::")));
    walk_module(graph, edge.target, &child_prefix, visited, out);
  }

  for pub_use in &module.pub_uses {
    let mut active_reexports = BTreeSet::new();
    emit_pub_use(graph, mod_id, prefix, pub_use, visited, &mut active_reexports, out);
  }
}

fn emit_item(
  graph: &ModuleGraph,
  defining_mod_id: usize,
  prefix: &[String],
  exported_name: &str,
  item: &PubItem,
  out: &mut BTreeSet<String>,
) {
  let full_name = format!("{}::{exported_name}", prefix.join("::"));
  out.insert(format!("{} {full_name}{}", item.kind_prefix, item.suffix));

  for member in &item.members {
    out.insert(format!("{} {full_name}::{}{}", member.kind_prefix, member.member_name, member.suffix));
  }

  if let Some(impl_members) = graph.modules[defining_mod_id].inherent_impls.get(&item.name) {
    for member in impl_members {
      out.insert(format!("{} {full_name}::{}{}", member.kind_prefix, member.member_name, member.suffix));
    }
  }
}

fn emit_pub_use(
  graph: &ModuleGraph,
  mod_id: usize,
  prefix: &[String],
  pub_use: &PubUse,
  visited: &mut BTreeSet<(usize, Vec<String>)>,
  active_reexports: &mut BTreeSet<(usize, Vec<String>)>,
  out: &mut BTreeSet<String>,
) {
  if !active_reexports.insert((mod_id, pub_use.segments.clone())) {
    return;
  }
  let resolved = resolve_segments(graph, mod_id, &pub_use.segments);
  match &pub_use.kind {
    | UseKind::Glob => {
      if let Some(&target_mod) = resolved.as_ref().and_then(|segs| graph.by_path.get(segs)) {
        walk_module(graph, target_mod, prefix, visited, out);
      } else {
        out.insert(format!("pub use {}::* = {}::*", prefix.join("::"), pub_use.segments.join("::")));
      }
    },
    | UseKind::Single { alias } => {
      if let Some(target_segs) = resolved {
        if let Some(&target_mod) = graph.by_path.get(&target_segs) {
          let mut child_prefix = prefix.to_vec();
          child_prefix.push(alias.clone());
          out.insert(format!("pub mod {}", child_prefix.join("::")));
          walk_module(graph, target_mod, &child_prefix, visited, out);
          return;
        }
        if target_segs.len() >= 2 {
          let parent_segs = &target_segs[..target_segs.len() - 1];
          let item_name = &target_segs[target_segs.len() - 1];
          if let Some(&target_mod) = graph.by_path.get(parent_segs) {
            if let Some(item) = graph.modules[target_mod].pub_items.iter().find(|i| i.name == *item_name) {
              emit_item(graph, target_mod, prefix, alias, item, out);
              return;
            }
            if let Some(reexport) = graph.modules[target_mod]
              .pub_uses
              .iter()
              .find(|u| matches!(&u.kind, UseKind::Single { alias: a } if a == item_name))
            {
              let nested =
                PubUse { segments: reexport.segments.clone(), kind: UseKind::Single { alias: alias.clone() } };
              emit_pub_use(graph, target_mod, prefix, &nested, visited, active_reexports, out);
              return;
            }
          }
        }
      }
      out.insert(format!("pub use {}::{alias} = {}", prefix.join("::"), pub_use.segments.join("::")));
    },
  }
}

fn resolve_segments(graph: &ModuleGraph, mod_id: usize, segments: &[String]) -> Option<Vec<String>> {
  if segments.is_empty() {
    return None;
  }
  let mut base = match segments[0].as_str() {
    | "crate" => vec![graph.crate_name.clone()],
    | "self" => graph.modules[mod_id].path.clone(),
    | "super" => {
      let mut p = graph.modules[mod_id].path.clone();
      if p.len() > 1 {
        p.pop();
      }
      p
    },
    | first => {
      let mut local = graph.modules[mod_id].path.clone();
      local.push(first.to_string());
      if is_known_prefix(graph, &local) {
        let mut out = graph.modules[mod_id].path.clone();
        out.extend(segments.iter().cloned());
        return Some(out);
      }
      return None;
    },
  };

  for seg in &segments[1..] {
    if seg == "super" {
      if base.len() > 1 {
        base.pop();
      }
    } else if seg != "self" {
      base.push(seg.clone());
    }
  }
  Some(base)
}

fn is_known_prefix(graph: &ModuleGraph, candidate: &[String]) -> bool {
  if graph.by_path.contains_key(candidate) {
    return true;
  }
  if candidate.len() >= 2
    && let Some(&parent_id) = graph.by_path.get(&candidate[..candidate.len() - 1])
  {
    let name = &candidate[candidate.len() - 1];
    return graph.modules[parent_id].pub_items.iter().any(|i| i.name == *name)
      || graph.modules[parent_id]
        .pub_uses
        .iter()
        .any(|u| matches!(&u.kind, UseKind::Single { alias } if alias == name));
  }
  false
}
