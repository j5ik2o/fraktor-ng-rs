//! The host DNS configuration read from the operating system.

#[cfg(all(test, not(windows)))]
#[path = "system_host_dns_configuration_test.rs"]
mod tests;
