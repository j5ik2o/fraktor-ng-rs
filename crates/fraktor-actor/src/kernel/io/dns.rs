//! Parsers for the DNS name server and resolver configuration.

mod dns_settings;
mod nameserver_address_error;
mod resolv_conf;
mod resolv_conf_parse_error;
mod resolv_conf_parser;

pub use dns_settings::DnsSettings;
pub use nameserver_address_error::NameserverAddressError;
pub use resolv_conf::ResolvConf;
pub use resolv_conf_parse_error::ResolvConfParseError;
pub use resolv_conf_parser::ResolvConfParser;
