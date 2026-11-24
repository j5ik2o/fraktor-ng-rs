//! The error a DNS wire format parser reports.

use core::fmt;

/// The reason a DNS message, or a part of one, could not be read.
///
/// Mirrors the exceptions that `org.apache.pekko.io.dns.internal.DomainName`,
/// `Question`, `RecordTypeSerializer`, `RecordClassSerializer`, `ResourceRecord`
/// and `Message` throw while reading a message: a read past the end of the
/// message (`NoSuchElementException`), and the rejections of malformed data
/// (`IllegalArgumentException`, and `RuntimeException` for a record class).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireFormatError {
  /// The message ended before the value being read was complete.
  Truncated,
  /// A domain name followed more than 16 compression pointers, so its pointers
  /// probably form a loop.
  TooManyCompressionPointers,
  /// A domain name holds a label whose two high bits are `01` or `10`, which
  /// RFC 1035 section 4.1.4 reserves. The value is the label type, `1` or `2`.
  ReservedLabelType(u8),
  /// A domain name's dotted text grew longer than 255 characters, the
  /// original's limit. It is not RFC 1035's limit of 255 octets on the wire.
  NameTooLong,
  /// A question names a record type code that is not a known record type.
  UnknownRecordType(u16),
  /// A question names a record class code that is not a known record class.
  UnknownRecordClass(u16),
  /// A resource record holds a negative time to live.
  NegativeTtl(i32),
}

impl fmt::Display for WireFormatError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::Truncated => formatter.write_str("DNS message ended before the value was complete"),
      | Self::TooManyCompressionPointers => {
        formatter.write_str("domain name has more than 16 compression pointers, probable pointer loop")
      },
      | Self::ReservedLabelType(label_type) => {
        write!(formatter, "domain name has an unsupported label type [{label_type}]")
      },
      | Self::NameTooLong => formatter.write_str("domain name is longer than 255 characters"),
      | Self::UnknownRecordType(code) => write!(formatter, "illegal id [{code}] for a DNS record type"),
      | Self::UnknownRecordClass(code) => write!(formatter, "unexpected DNS record class {code}"),
      | Self::NegativeTtl(seconds) => write!(formatter, "DNS record time to live {seconds} is negative"),
    }
  }
}
