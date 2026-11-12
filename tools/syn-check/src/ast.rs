//! Position helpers shared by the rules.

use proc_macro2::{Delimiter, TokenTree};
use quote::ToTokens;
use syn::{Attribute, Item, spanned::Spanned};

use crate::position::Position;

/// The first token of `node`, outer attributes and doc comments included.
pub fn start<T: ToTokens>(node: &T) -> Position {
  node.span().start().into()
}

/// The line of the last token of `node`.
pub fn end_line<T: ToTokens>(node: &T) -> usize {
  node.span().end().line
}

/// The first token of `node` after its outer attributes and doc comments.
///
/// rustc reports an item at this position: the visibility if there is one,
/// otherwise the keyword. The old lints took their line numbers from there.
pub fn decl_start<T: ToTokens>(node: &T) -> Position {
  let mut tokens = node.to_token_stream().into_iter().peekable();
  while let Some(token) = tokens.next() {
    let is_attribute = matches!(&token, TokenTree::Punct(punct) if punct.as_char() == '#')
      && matches!(tokens.peek(), Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Bracket);
    if is_attribute {
      tokens.next();
      continue;
    }
    return token.span().start().into();
  }
  start(node)
}

/// The outer attributes of an item. `Item::Verbatim` has none.
pub fn item_attrs(item: &Item) -> &[Attribute] {
  match item {
    | Item::Const(item) => &item.attrs,
    | Item::Enum(item) => &item.attrs,
    | Item::ExternCrate(item) => &item.attrs,
    | Item::Fn(item) => &item.attrs,
    | Item::ForeignMod(item) => &item.attrs,
    | Item::Impl(item) => &item.attrs,
    | Item::Macro(item) => &item.attrs,
    | Item::Mod(item) => &item.attrs,
    | Item::Static(item) => &item.attrs,
    | Item::Struct(item) => &item.attrs,
    | Item::Trait(item) => &item.attrs,
    | Item::TraitAlias(item) => &item.attrs,
    | Item::Type(item) => &item.attrs,
    | Item::Union(item) => &item.attrs,
    | Item::Use(item) => &item.attrs,
    | _ => &[],
  }
}
