//! Parsers for the DNS name server and resolver configuration, and for DNS
//! messages in their wire format.

mod dns_settings;
mod domain_name;
mod message;
mod message_flags;
mod nameserver_address_error;
mod question;
mod record_class;
mod record_class_code;
mod record_type;
mod record_type_code;
mod resolv_conf;
mod resolv_conf_parse_error;
mod resolv_conf_parser;
mod resource_record;
mod wire_format_error;
mod wire_reader;

pub use dns_settings::DnsSettings;
pub use domain_name::DomainName;
pub use message::Message;
pub use message_flags::MessageFlags;
pub use nameserver_address_error::NameserverAddressError;
pub use question::Question;
pub use record_class::RecordClass;
pub use record_class_code::RecordClassCode;
pub use record_type::RecordType;
pub use record_type_code::RecordTypeCode;
pub use resolv_conf::ResolvConf;
pub use resolv_conf_parse_error::ResolvConfParseError;
pub use resolv_conf_parser::ResolvConfParser;
pub use resource_record::ResourceRecord;
pub use wire_format_error::WireFormatError;
pub use wire_reader::WireReader;
