//! Where and how to reach a remote endpoint.
//!
//! "How to reach it" is deliberately a separate axis from "what protocol is
//! spoken once connected". Today the only transport is a direct connection to a
//! [`Target`]; tunnelling the byte stream over SSH would be added here rather
//! than inside each protocol crate.

/// A network endpoint: a host (name or IP) and a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
}

impl Target {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }
}
