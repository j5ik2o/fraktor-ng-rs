//! A domain name read from a DNS message.

use alloc::string::String;
use core::fmt;

use super::{WireFormatError, WireReader};

#[cfg(test)]
#[path = "domain_name_test.rs"]
mod tests;

// RFC 1035 section 2.3.4 limits a name to 255 octets on the wire, so a
// well-formed name never holds more than a handful of compression pointers.
// Bounding the pointers followed makes `parse` finish even when the pointers
// form a cycle.
const MAX_POINTER_HOPS: usize = 16;
// The original's `MaxNameLength`, compared with the length of the dotted text
// it builds. This is not the RFC's 255-octet wire limit, which also counts each
// label's length byte and the terminating zero: a dotted name of 255
// characters takes 257 octets on the wire and is still accepted, as in the
// original.
const MAX_NAME_LENGTH: usize = 255;
const POINTER_TAG: u8 = 0xC0;

/// A domain name in its dotted text form, such as `www.example.com`.
///
/// Mirrors the `String` that `org.apache.pekko.io.dns.internal.DomainName.parse`
/// returns. Writing a name into a message (`DomainName.write`) is not part of
/// this port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DomainName {
  text: String,
}

impl DomainName {
  /// Reads a domain name at the reader's position, following compression
  /// pointers into the rest of the message.
  ///
  /// Mirrors `DomainName.parse(it, msg)`. The labels are joined with `.`, and
  /// the name ends at a zero length byte. A length byte whose two high bits are
  /// set is a compression pointer: the name goes on at the 14-bit offset it
  /// holds. The reader moves past the name, or past the first compression
  /// pointer when the name has one.
  ///
  /// Each byte of a label becomes one character the way Scala's `Byte.toChar`
  /// makes it: a byte below `0x80` is the character with that code, and a byte
  /// from `0x80` up is sign-extended, so `0xE9` is `U+FFE9`.
  ///
  /// # Errors
  ///
  /// - [`WireFormatError::Truncated`] when the message ends before the name
  ///   does, including a label that runs past the end of the message.
  /// - [`WireFormatError::TooManyCompressionPointers`] when the name follows
  ///   more than 16 compression pointers, as a pointer to itself or a cycle of
  ///   pointers does.
  /// - [`WireFormatError::ReservedLabelType`] when a length byte's two high
  ///   bits are `01` or `10`.
  /// - [`WireFormatError::NameTooLong`] when the dotted text grows past 255
  ///   characters, the original's limit. The RFC 1035 limit of 255 octets on
  ///   the wire, which counts the length bytes and the terminating zero, is not
  ///   checked.
  pub fn parse(reader: &mut WireReader<'_>) -> Result<Self, WireFormatError> {
    let mut text = String::new();
    let mut length = 0_usize;
    let mut hops = 0_usize;
    let mut followed: Option<WireReader<'_>> = None;
    loop {
      let current = match followed.as_mut() {
        | Some(followed) => followed,
        | None => &mut *reader,
      };
      let length_byte = current.read_u8()?;
      if length_byte == 0 {
        return Ok(Self { text });
      }
      match length_byte & POINTER_TAG {
        | POINTER_TAG => {
          hops += 1;
          if hops > MAX_POINTER_HOPS {
            return Err(WireFormatError::TooManyCompressionPointers);
          }
          let offset = (usize::from(length_byte & !POINTER_TAG) << 8) | usize::from(current.read_u8()?);
          followed = Some(current.jump_to(offset));
        },
        | 0 => {
          if !text.is_empty() {
            text.push('.');
            length += 1;
          }
          for &byte in current.read_up_to(usize::from(length_byte)) {
            text.push(scala_byte_to_char(byte));
            length += 1;
          }
          if length > MAX_NAME_LENGTH {
            return Err(WireFormatError::NameTooLong);
          }
        },
        | reserved => return Err(WireFormatError::ReservedLabelType(reserved >> 6)),
      }
    }
  }

  /// Returns the name in its dotted text form.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.text
  }
}

impl fmt::Display for DomainName {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.text)
  }
}

// Scala's `Byte.toChar` widens the signed byte to a 16-bit code unit, so a byte
// from `0x80` up becomes `U+FF80`..`U+FFFF`. None of those is a surrogate, so
// `char::from_u32` always succeeds.
fn scala_byte_to_char(byte: u8) -> char {
  if byte < 0x80 {
    char::from(byte)
  } else {
    char::from_u32(0xFF00 | u32::from(byte)).unwrap_or(char::REPLACEMENT_CHARACTER)
  }
}
