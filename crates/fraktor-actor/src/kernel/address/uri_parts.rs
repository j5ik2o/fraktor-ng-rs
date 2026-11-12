//! Parsed components of an absolute or relative URI as understood by
//! `java.net.URI`.

#[cfg(test)]
#[path = "uri_parts_test.rs"]
mod tests;

/// The parts of an absolute URI that the address and path parsers consume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UriParts<'a> {
  scheme: &'a str,
  user_info: Option<&'a str>,
  host: &'a str,
  port: Option<u16>,
  raw_path: &'a str,
}

impl<'a> UriParts<'a> {
  /// Parses an absolute `scheme://authority/path` URI or returns `None`.
  ///
  /// The grammar follows RFC 2396, matching the rules `java.net.URI`
  /// applies to server-based authorities and hierarchical paths.
  pub(crate) fn parse(input: &'a str) -> Option<Self> {
    let (scheme, rest) = input.split_once("://")?;
    if !is_scheme(scheme) {
      return None;
    }
    let rest = strip_query_and_fragment(rest);
    let (authority, raw_path) = match rest.find('/') {
      | Some(index) => (&rest[..index], &rest[index..]),
      | None => (rest, ""),
    };
    let (user_info, host_port) = match authority.rfind('@') {
      | Some(index) => (Some(&authority[..index]), &authority[index + 1..]),
      | None => (None, authority),
    };
    if let Some(info) = user_info {
      if !is_user_info(info) {
        return None;
      }
    }
    let (host, port) = split_host_port(host_port)?;
    if !is_host(host) {
      return None;
    }
    if !is_raw_path(raw_path) {
      return None;
    }
    Some(Self { scheme, user_info, host, port, raw_path })
  }

  /// Returns the URI scheme.
  pub(crate) fn scheme(&self) -> &'a str {
    self.scheme
  }

  /// Returns the user information preceding the host, if any.
  pub(crate) fn user_info(&self) -> Option<&'a str> {
    self.user_info
  }

  /// Returns the host authority component.
  pub(crate) fn host(&self) -> &'a str {
    self.host
  }

  /// Returns the port, if the authority carried one.
  pub(crate) fn port(&self) -> Option<u16> {
    self.port
  }

  /// Returns the raw path beginning with `/`, or an empty string.
  pub(crate) fn raw_path(&self) -> &'a str {
    self.raw_path
  }
}

fn strip_query_and_fragment(rest: &str) -> &str {
  let end = rest.find(['?', '#']).unwrap_or(rest.len());
  &rest[..end]
}

/// The raw path and fragment of a relative URI reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RelativeUriParts<'a> {
  raw_path: &'a str,
  raw_fragment: Option<&'a str>,
}

impl<'a> RelativeUriParts<'a> {
  /// Parses a relative URI reference or returns `None`.
  ///
  /// Returns `None` for inputs carrying a scheme, for network-path
  /// references starting with `//`, and for inputs that violate the
  /// RFC 2396 character rules.
  pub(crate) fn parse(input: &'a str) -> Option<Self> {
    let scheme_end = input.find(['/', '?', '#']).unwrap_or(input.len());
    if input[..scheme_end].contains(':') {
      return None;
    }
    if input.starts_with("//") {
      return None;
    }
    let (before_fragment, raw_fragment) = match input.find('#') {
      | Some(index) => (&input[..index], Some(&input[index + 1..])),
      | None => (input, None),
    };
    let raw_path = match before_fragment.find('?') {
      | Some(index) => &before_fragment[..index],
      | None => before_fragment,
    };
    if !is_raw_path(raw_path) {
      return None;
    }
    if let Some(fragment) = raw_fragment {
      if !is_uric(fragment) {
        return None;
      }
    }
    Some(Self { raw_path, raw_fragment })
  }

  /// Returns the raw path, which is not percent-decoded.
  pub(crate) fn raw_path(&self) -> &'a str {
    self.raw_path
  }

  /// Returns the raw fragment, which is not percent-decoded, if present.
  pub(crate) fn raw_fragment(&self) -> Option<&'a str> {
    self.raw_fragment
  }
}

fn split_host_port(authority: &str) -> Option<(&str, Option<u16>)> {
  match authority.rsplit_once(':') {
    | Some((host, port)) => {
      if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
      }
      let port: u16 = port.parse().ok()?;
      Some((host, Some(port)))
    },
    | None => Some((authority, None)),
  }
}

fn is_scheme(scheme: &str) -> bool {
  let mut bytes = scheme.bytes();
  match bytes.next() {
    | Some(first) if first.is_ascii_alphabetic() => {},
    | _ => return false,
  }
  bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'-' || byte == b'.')
}

fn is_user_info(info: &str) -> bool {
  info.bytes().all(is_user_info_byte)
}

fn is_user_info_byte(byte: u8) -> bool {
  is_unreserved(byte)
    || byte == b';'
    || byte == b':'
    || byte == b'&'
    || byte == b'='
    || byte == b'+'
    || byte == b'$'
    || byte == b','
}

fn is_host(host: &str) -> bool {
  !host.is_empty() && (is_hostname(host) || is_ipv4(host))
}

fn is_hostname(host: &str) -> bool {
  host.split('.').all(is_hostname_label)
}

fn is_hostname_label(label: &str) -> bool {
  let bytes = label.as_bytes();
  if bytes.is_empty() {
    return false;
  }
  let first = bytes[0];
  let last = bytes[bytes.len() - 1];
  if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
    return false;
  }
  bytes.iter().all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn is_ipv4(host: &str) -> bool {
  let labels: Vec<&str> = host.split('.').collect();
  labels.len() == 4 && labels.iter().all(|label| !label.is_empty() && label.parse::<u8>().is_ok())
}

/// Returns whether `path` consists only of `/`, path characters, or `%`
/// followed by two hexadecimal digits, as RFC 2396 defines `escaped`.
fn is_raw_path(path: &str) -> bool {
  all_chars_or_escaped(path, |byte| byte == b'/' || is_path_char(byte))
}

fn is_path_char(byte: u8) -> bool {
  is_unreserved(byte)
    || byte == b':'
    || byte == b'@'
    || byte == b'&'
    || byte == b'='
    || byte == b'+'
    || byte == b'$'
    || byte == b','
    || byte == b';'
}

/// Returns whether `text` consists only of RFC 2396 `uric` characters or of
/// `%` followed by two hexadecimal digits.
fn is_uric(text: &str) -> bool {
  all_chars_or_escaped(text, is_uric_char)
}

fn is_uric_char(byte: u8) -> bool {
  is_unreserved(byte)
    || byte == b';'
    || byte == b'/'
    || byte == b'?'
    || byte == b':'
    || byte == b'@'
    || byte == b'&'
    || byte == b'='
    || byte == b'+'
    || byte == b'$'
    || byte == b','
}

fn is_unreserved(byte: u8) -> bool {
  byte.is_ascii_alphanumeric()
    || byte == b'-'
    || byte == b'_'
    || byte == b'.'
    || byte == b'!'
    || byte == b'~'
    || byte == b'*'
    || byte == b'\''
    || byte == b'('
    || byte == b')'
}

/// Returns whether `s` consists only of bytes accepted by `allowed` or of
/// `%` followed by two hexadecimal digits, as RFC 2396 defines `escaped`.
fn all_chars_or_escaped(s: &str, allowed: fn(u8) -> bool) -> bool {
  let bytes = s.as_bytes();
  let mut position = 0;
  while position < bytes.len() {
    let byte = bytes[position];
    if byte == b'%' {
      if position + 2 < bytes.len()
        && bytes[position + 1].is_ascii_hexdigit()
        && bytes[position + 2].is_ascii_hexdigit()
      {
        position += 3;
        continue;
      }
      return false;
    }
    if !allowed(byte) {
      return false;
    }
    position += 1;
  }
  true
}
