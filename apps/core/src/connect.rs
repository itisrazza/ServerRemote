//! What the connect form and saved sessions open a connection with.

use server_remote_core::Target;

use crate::Protocol;
use crate::persistence::sessions::SavedSession;

/// Everything needed to either open a connection tab or save a session.
/// `name` and `group` are only meaningful for saving — the connect flow
/// derives a tab name from the protocol and hostname instead.
#[derive(Debug, Clone)]
pub struct Request {
    pub protocol: Protocol,
    pub name: String,
    pub group: String,
    pub hostname: String,
    /// Only iLO asks for one; other protocols ignore it.
    pub username: String,
    pub password: String,
}

impl From<&SavedSession> for Request {
    fn from(session: &SavedSession) -> Self {
        Request {
            protocol: session.protocol,
            name: session.name.clone(),
            group: session.group.clone(),
            hostname: session.hostname.clone(),
            username: session.username.clone(),
            password: session.password.clone(),
        }
    }
}

/// Split a `host[:port]` string, falling back to the protocol's default port.
pub fn parse_target(hostname: &str, protocol: Protocol) -> Target {
    let default_port = protocol.default_port();
    if hostname.is_empty() {
        return Target::new("localhost", default_port);
    }
    match hostname.rsplit_once(':') {
        Some((host, port)) => match port.parse::<u16>() {
            Ok(port) => Target::new(host, port),
            Err(_) => Target::new(hostname, default_port),
        },
        None => Target::new(hostname, default_port),
    }
}

/// Builds a [`SavedSession`] from a connect-tab
/// request, falling back to the hostname and then the protocol label when the
/// user left the name field blank.
pub fn saved_session_from_request(request: Request) -> SavedSession {
    let name = if !request.name.trim().is_empty() {
        request.name.trim().to_owned()
    } else if !request.hostname.is_empty() {
        request.hostname.clone()
    } else {
        request.protocol.to_string()
    };

    SavedSession {
        id: 0,
        name,
        group: request.group,
        protocol: request.protocol,
        hostname: request.hostname,
        username: request.username,
        password: request.password,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_name_saves_as_the_hostname() {
        let request = Request {
            protocol: Protocol::Vnc,
            name: "   ".to_owned(),
            group: String::new(),
            hostname: "nas.local".to_owned(),
            username: String::new(),
            password: String::new(),
        };
        assert_eq!(saved_session_from_request(request).name, "nas.local");
    }

    #[test]
    fn empty_hostname_falls_back_to_localhost_and_default_port() {
        let target = parse_target("", Protocol::Vnc);
        assert_eq!(target, Target::new("localhost", 5900));
    }

    #[test]
    fn host_without_port_uses_the_protocol_default() {
        let target = parse_target("example.com", Protocol::Ilo);
        assert_eq!(target, Target::new("example.com", 443));
    }

    #[test]
    fn host_with_valid_port_overrides_the_default() {
        let target = parse_target("example.com:2222", Protocol::Vnc);
        assert_eq!(target, Target::new("example.com", 2222));
    }

    #[test]
    fn host_with_unparsable_port_keeps_the_whole_string_as_host() {
        let target = parse_target("example.com:not-a-port", Protocol::Vnc);
        assert_eq!(target, Target::new("example.com:not-a-port", 5900));
    }

    #[test]
    fn host_with_out_of_range_port_keeps_the_whole_string_as_host() {
        let target = parse_target("example.com:99999", Protocol::Vnc);
        assert_eq!(target, Target::new("example.com:99999", 5900));
    }
}
