// SPDX-License-Identifier: Apache-2.0

//! Signature formatting for extracted public API items.

use quote::ToTokens;
use syn::{Fields, Generics, ImplItem, Item, ItemTrait, ReturnType, Signature, TraitItem, Type, Variant};

use super::{MemberSignature, PubItem, has_cfg_test, is_public};

pub(crate) fn extract_pub_item(item: &Item) -> Option<PubItem> {
  match item {
    | Item::Struct(s) if is_public(&s.vis) => Some(PubItem {
      name: s.ident.to_string(),
      kind_prefix: "pub struct".to_string(),
      suffix: generics_and_where(&s.generics),
      members: extract_pub_fields(&s.fields),
    }),
    | Item::Enum(e) if is_public(&e.vis) => Some(PubItem {
      name: e.ident.to_string(),
      kind_prefix: "pub enum".to_string(),
      suffix: generics_and_where(&e.generics),
      members: e.variants.iter().filter(|variant| !has_cfg_test(&variant.attrs)).map(format_variant).collect(),
    }),
    | Item::Union(u) if is_public(&u.vis) => Some(PubItem {
      name: u.ident.to_string(),
      kind_prefix: "pub union".to_string(),
      suffix: generics_and_where(&u.generics),
      members: u
        .fields
        .named
        .iter()
        .filter(|field| is_public(&field.vis) && !has_cfg_test(&field.attrs))
        .filter_map(|field| {
          let ident = field.ident.as_ref()?.to_string();
          Some(MemberSignature {
            kind_prefix: "pub".to_string(),
            member_name: ident,
            suffix: format!(": {}", compact_tokens(&field.ty)),
          })
        })
        .collect(),
    }),
    | Item::Trait(t) if is_public(&t.vis) => {
      let kind_prefix = if t.unsafety.is_some() { "pub unsafe trait" } else { "pub trait" };
      Some(PubItem {
        name: t.ident.to_string(),
        kind_prefix: kind_prefix.to_string(),
        suffix: trait_suffix(t),
        members: t.items.iter().filter_map(extract_trait_item).collect(),
      })
    },
    | Item::Type(t) if is_public(&t.vis) => Some(PubItem {
      name: t.ident.to_string(),
      kind_prefix: "pub type".to_string(),
      suffix: format!("{} = {}", compact_generics(&t.generics), compact_tokens(&t.ty)),
      members: Vec::new(),
    }),
    | Item::Fn(f) if is_public(&f.vis) => Some(PubItem {
      name: f.sig.ident.to_string(),
      kind_prefix: fn_prefix(&f.sig),
      suffix: fn_suffix(&f.sig),
      members: Vec::new(),
    }),
    | Item::Const(c) if is_public(&c.vis) => Some(PubItem {
      name: c.ident.to_string(),
      kind_prefix: "pub const".to_string(),
      suffix: format!(": {}", compact_tokens(&c.ty)),
      members: Vec::new(),
    }),
    | Item::Static(s) if is_public(&s.vis) => {
      let kind_prefix = match s.mutability {
        | syn::StaticMutability::Mut(_) => "pub static mut",
        | _ => "pub static",
      };
      Some(PubItem {
        name: s.ident.to_string(),
        kind_prefix: kind_prefix.to_string(),
        suffix: format!(": {}", compact_tokens(&s.ty)),
        members: Vec::new(),
      })
    },
    | _ => None,
  }
}

pub(crate) fn extract_pub_impl_item(item: &ImplItem) -> Option<MemberSignature> {
  match item {
    | ImplItem::Fn(f) if is_public(&f.vis) => Some(MemberSignature {
      kind_prefix: fn_prefix(&f.sig),
      member_name: f.sig.ident.to_string(),
      suffix: fn_suffix(&f.sig),
    }),
    | ImplItem::Const(c) if is_public(&c.vis) => Some(MemberSignature {
      kind_prefix: "pub const".to_string(),
      member_name: c.ident.to_string(),
      suffix: format!(": {}", compact_tokens(&c.ty)),
    }),
    | ImplItem::Type(t) if is_public(&t.vis) => Some(MemberSignature {
      kind_prefix: "pub type".to_string(),
      member_name: t.ident.to_string(),
      suffix: format!("{} = {}", compact_generics(&t.generics), compact_tokens(&t.ty)),
    }),
    | _ => None,
  }
}

pub(crate) fn impl_target_ident(ty: &Type) -> Option<String> {
  match ty {
    | Type::Path(type_path) => type_path.path.segments.last().map(|seg| seg.ident.to_string()),
    | Type::Group(group) => impl_target_ident(&group.elem),
    | Type::Paren(paren) => impl_target_ident(&paren.elem),
    | _ => None,
  }
}

fn extract_pub_fields(fields: &Fields) -> Vec<MemberSignature> {
  match fields {
    | Fields::Named(named) => named
      .named
      .iter()
      .filter(|field| is_public(&field.vis) && !has_cfg_test(&field.attrs))
      .filter_map(|field| {
        let ident = field.ident.as_ref()?.to_string();
        Some(MemberSignature {
          kind_prefix: "pub".to_string(),
          member_name: ident,
          suffix: format!(": {}", compact_tokens(&field.ty)),
        })
      })
      .collect(),
    | Fields::Unnamed(unnamed) => unnamed
      .unnamed
      .iter()
      .enumerate()
      .filter(|(_, field)| is_public(&field.vis) && !has_cfg_test(&field.attrs))
      .map(|(idx, field)| MemberSignature {
        kind_prefix: "pub".to_string(),
        member_name: idx.to_string(),
        suffix: format!(": {}", compact_tokens(&field.ty)),
      })
      .collect(),
    | Fields::Unit => Vec::new(),
  }
}

fn format_variant(variant: &Variant) -> MemberSignature {
  let suffix = match &variant.fields {
    | Fields::Unit => String::new(),
    | Fields::Unnamed(u) => {
      let inner: Vec<String> = u.unnamed.iter().map(|f| compact_tokens(&f.ty)).collect();
      format!("({})", inner.join(", "))
    },
    | Fields::Named(n) => {
      let inner: Vec<String> =
        n.named.iter().filter_map(|f| Some(format!("{}: {}", f.ident.as_ref()?, compact_tokens(&f.ty)))).collect();
      format!(" {{ {} }}", inner.join(", "))
    },
  };
  MemberSignature { kind_prefix: "pub".to_string(), member_name: variant.ident.to_string(), suffix }
}

fn extract_trait_item(item: &TraitItem) -> Option<MemberSignature> {
  match item {
    | TraitItem::Fn(f) if !has_cfg_test(&f.attrs) => Some(MemberSignature {
      kind_prefix: fn_prefix(&f.sig),
      member_name: f.sig.ident.to_string(),
      suffix: fn_suffix(&f.sig),
    }),
    | TraitItem::Type(t) if !has_cfg_test(&t.attrs) => Some(MemberSignature {
      kind_prefix: "pub type".to_string(),
      member_name: t.ident.to_string(),
      suffix: compact_generics(&t.generics),
    }),
    | TraitItem::Const(c) if !has_cfg_test(&c.attrs) => Some(MemberSignature {
      kind_prefix: "pub const".to_string(),
      member_name: c.ident.to_string(),
      suffix: format!(": {}", compact_tokens(&c.ty)),
    }),
    | _ => None,
  }
}

fn fn_prefix(sig: &Signature) -> String {
  let mut parts = vec!["pub"];
  if sig.constness.is_some() {
    parts.push("const");
  }
  if sig.asyncness.is_some() {
    parts.push("async");
  }
  if sig.unsafety.is_some() {
    parts.push("unsafe");
  }
  parts.push("fn");
  parts.join(" ")
}

fn fn_suffix(sig: &Signature) -> String {
  let generics = compact_generics(&sig.generics);
  let inputs: Vec<String> = sig.inputs.iter().map(compact_tokens).collect();
  let ret = match &sig.output {
    | ReturnType::Default => String::new(),
    | ReturnType::Type(_, ty) => format!(" -> {}", compact_tokens(ty)),
  };
  format!("{generics}({}){ret}{}", inputs.join(", "), where_suffix(&sig.generics))
}

fn generics_and_where(generics: &Generics) -> String {
  format!("{}{}", compact_generics(generics), where_suffix(generics))
}

/// The generics, the supertraits, and the where clause of a trait, as in
/// `<T>: Clone + Send where T: Copy`.
fn trait_suffix(item: &ItemTrait) -> String {
  let supertraits = if item.supertraits.is_empty() {
    String::new()
  } else {
    let bounds: Vec<String> = item.supertraits.iter().map(compact_tokens).collect();
    format!(": {}", bounds.join(" + "))
  };
  format!("{}{supertraits}{}", compact_generics(&item.generics), where_suffix(&item.generics))
}

/// ` where A: B, ..` when the generics have a where clause, so that a change
/// of the bounds changes the line.
fn where_suffix(generics: &Generics) -> String {
  match &generics.where_clause {
    | Some(clause) if !clause.predicates.is_empty() => {
      let predicates: Vec<String> = clause.predicates.iter().map(compact_tokens).collect();
      format!(" where {}", predicates.join(", "))
    },
    | _ => String::new(),
  }
}

fn compact_generics(generics: &Generics) -> String {
  if generics.params.is_empty() {
    return String::new();
  }
  let params: Vec<String> = generics.params.iter().map(compact_tokens).collect();
  format!("<{}>", params.join(", "))
}

fn compact_tokens(tokens: &impl ToTokens) -> String {
  let raw = tokens.to_token_stream().to_string();
  normalize_rust_tokens(&raw)
}

fn normalize_rust_tokens(raw: &str) -> String {
  let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
  let s = collapsed
    .replace(" :: ", "::")
    .replace(" < ", "<")
    .replace(" >", ">")
    .replace(" , ", ", ")
    .replace(" : ", ": ")
    .replace("& '", "&'")
    .replace("& mut ", "&mut ")
    .replace("( ", "(")
    .replace(" )", ")")
    .replace("[ ", "[")
    .replace(" ]", "]")
    .replace(" ; ", "; ")
    .replace(": & ", ": &")
    .replace("-> & ", "-> &")
    .replace("(& ", "(&")
    .replace(", & ", ", &")
    .replace("<& ", "<&")
    .replace("FnOnce (", "FnOnce(")
    .replace("FnMut (", "FnMut(")
    .replace("Fn (", "Fn(")
    .replace("? ", "?");
  if let Some(rest) = s.strip_prefix("& ") { format!("&{rest}") } else { s }
}
