//! The iLO's HTTPS JSON API: login, the remote console parameters
//! (`rc_info`) and logout. See section 1 of `docs/protocols/ilo4.md`.

use serde_json::Value;
use server_remote_core::{ConnectError, Target, TlsPolicy};

use crate::http::{self, Response};

/// A logged-in web session.
#[derive(Debug, Clone)]
pub struct Session {
    /// The session key: sent with every request, and (obfuscated) in the
    /// console negotiation.
    pub key: String,
    /// The name of the cookie that carries `key`: `sessionKey` on ProLiant
    /// iLOs, `MPID` on Integrity ones.
    cookie: &'static str,
    /// Privileges reported at login.
    pub privileges: Privileges,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Privileges {
    pub remote_console: bool,
    pub virtual_media: bool,
    pub reset: bool,
}

/// The v1 remote console parameters from `GET /json/rc_info`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RcInfo {
    /// The KVM cipher key.
    pub enc_key: [u8; 16],
    /// `enc_key` as the hex text the iLO sent: the XOR pad that obfuscates
    /// the session key in the negotiation.
    pub enc_key_text: String,
    /// The command channel's cipher key.
    pub cmd_enc_key: [u8; 16],
    /// The port both the KVM and command connections use.
    pub rc_port: u16,
    pub features: Features,
    /// Something to call the server by, if the iLO knows one.
    pub server_name: Option<String>,
}

/// The `optional_features` flags that change what the client does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Features {
    pub encrypt_key: bool,
    pub encrypt_vm_key: bool,
    pub encrypt_cmd: bool,
}

impl Session {
    /// Logs in with a user name and password.
    pub async fn login(
        target: &Target,
        policy: &TlsPolicy,
        user: &str,
        password: &str,
    ) -> Result<Self, ConnectError> {
        let body = serde_json::json!({
            "method": "login",
            "user_login": user,
            "password": password,
        });
        let response = post(target, policy, "/json/login_session", &body, &[]).await?;
        match response.status {
            200 => {}
            401 | 403 => {
                return Err(ConnectError::Auth(
                    "the iLO rejected the user name or password".into(),
                ));
            }
            status => {
                return Err(ConnectError::Protocol(format!(
                    "login failed with HTTP {status}"
                )));
            }
        }
        let cookie = if response.cookie("sessionKey").is_some() {
            "sessionKey"
        } else {
            "MPID"
        };
        let info = json(&response)?;
        let key = info
            .get("session_key")
            .or_else(|| info.get("MPID"))
            .and_then(Value::as_str)
            .ok_or_else(|| ConnectError::Protocol("login reply has no session key".into()))?
            .to_owned();
        Ok(Session {
            key,
            cookie,
            privileges: Privileges {
                remote_console: flag(&info, "remote_cons_priv"),
                virtual_media: flag(&info, "virtual_media_priv"),
                reset: flag(&info, "reset_priv"),
            },
        })
    }

    /// Fetches the remote console parameters. Their keys are new for every
    /// login.
    pub async fn rc_info(
        &self,
        target: &Target,
        policy: &TlsPolicy,
    ) -> Result<RcInfo, ConnectError> {
        let response = http::request(
            target,
            policy,
            "GET",
            "/json/rc_info",
            &self.headers(&self.cookie_header()),
            None,
        )
        .await?;
        if response.status != 200 {
            return Err(ConnectError::Protocol(format!(
                "rc_info failed with HTTP {}",
                response.status
            )));
        }
        RcInfo::parse(&json(&response)?)
    }

    /// Ends the web session. iLO 4 ignores a logout without the session
    /// cookie, so it's always sent.
    pub async fn logout(&self, target: &Target, policy: &TlsPolicy) -> Result<(), ConnectError> {
        let body = serde_json::json!({ "method": "logout", "session_key": self.key });
        let cookie = self.cookie_header();
        let response = post(
            target,
            policy,
            "/json/login_session",
            &body,
            &self.headers(&cookie),
        )
        .await?;
        match response.status {
            200 => Ok(()),
            status => Err(ConnectError::Protocol(format!(
                "logout failed with HTTP {status}"
            ))),
        }
    }

    fn cookie_header(&self) -> String {
        format!("{}={}", self.cookie, self.key)
    }

    fn headers<'a>(&'a self, cookie: &'a str) -> [(&'a str, &'a str); 2] {
        [("Cookie", cookie), ("X-Auth-Token", &self.key)]
    }
}

impl RcInfo {
    fn parse(info: &Value) -> Result<Self, ConnectError> {
        let missing =
            |field: &str| ConnectError::Protocol(format!("rc_info has no usable {field}"));
        let text = |field: &str| info.get(field).and_then(Value::as_str);

        let enc_key_text = text("enc_key").ok_or_else(|| missing("enc_key"))?;
        let enc_key = parse_key(enc_key_text).ok_or_else(|| missing("enc_key"))?;
        let cmd_enc_key = text("cmd_enc_key")
            .and_then(parse_key)
            .ok_or_else(|| missing("cmd_enc_key"))?;
        let rc_port = match info.get("rc_port") {
            Some(Value::Number(port)) => port.as_u64(),
            Some(Value::String(port)) => port.parse().ok(),
            _ => None,
        }
        .and_then(|port| u16::try_from(port).ok())
        .ok_or_else(|| missing("rc_port"))?;

        let flags: Vec<&str> = text("optional_features")
            .unwrap_or_default()
            .split(';')
            .map(str::trim)
            .collect();
        let features = Features {
            encrypt_key: flags.contains(&"ENCRYPT_KEY"),
            encrypt_vm_key: flags.contains(&"ENCRYPT_VMKEY"),
            encrypt_cmd: flags.contains(&"ENCRYPT_CMD"),
        };

        // "null" (a string) and null both mean unset.
        let name = |field: &str| {
            text(field)
                .map(str::trim)
                .filter(|value| !value.is_empty() && *value != "null")
                .map(str::to_owned)
        };
        let server_name = name("server_name").or_else(|| name("ilo_fqdn"));

        Ok(RcInfo {
            enc_key,
            enc_key_text: enc_key_text.to_owned(),
            cmd_enc_key,
            rc_port,
            features,
            server_name,
        })
    }
}

async fn post(
    target: &Target,
    policy: &TlsPolicy,
    path: &str,
    body: &Value,
    headers: &[(&str, &str)],
) -> Result<Response, ConnectError> {
    let body =
        serde_json::to_vec(body).map_err(|error| ConnectError::Protocol(error.to_string()))?;
    http::request(target, policy, "POST", path, headers, Some(&body)).await
}

fn json(response: &Response) -> Result<Value, ConnectError> {
    serde_json::from_slice(&response.body)
        .map_err(|error| ConnectError::Protocol(format!("the iLO sent invalid JSON: {error}")))
}

/// Privileges come as `0`/`1` from iLO 4, but accept booleans too.
fn flag(info: &Value, field: &str) -> bool {
    match info.get(field) {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_u64().is_some_and(|value| value != 0),
        _ => false,
    }
}

/// 32 hex characters to 16 bytes.
fn parse_key(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32 {
        return None;
    }
    let mut key = [0u8; 16];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rc_info_as_firmware_2_82_sends_it() {
        let info = serde_json::json!({
            "enc_key": "000102030405060708090a0b0c0d0e0f", "enc_type": 0, "rc_port": 17990,
            "vm_key": "000102030405060708090a0b0c0d0e0f", "vm_port": 17988,
            "cmd_enc_key": "ffeeddccbbaa99887766554433221100", "protocol_version": "1.1",
            "optional_features": "ENCRYPT_KEY;ENCRYPT_VMKEY;ENCRYPT_CMD",
            "server_name": "", "ilo_fqdn": "ilo.example", "blade": 0, "bay": null, "enclosure": null
        });
        let rc = RcInfo::parse(&info).unwrap();
        assert_eq!(rc.enc_key, core::array::from_fn(|i| i as u8));
        assert_eq!(rc.enc_key_text, "000102030405060708090a0b0c0d0e0f");
        assert_eq!(rc.cmd_enc_key[0], 0xff);
        assert_eq!(rc.rc_port, 17990);
        assert_eq!(
            rc.features,
            Features {
                encrypt_key: true,
                encrypt_vm_key: true,
                encrypt_cmd: true,
            }
        );
        assert_eq!(rc.server_name.as_deref(), Some("ilo.example"));
    }

    #[test]
    fn rc_info_without_keys_is_a_protocol_error() {
        let info = serde_json::json!({ "rc_port": 17990 });
        assert!(matches!(
            RcInfo::parse(&info),
            Err(ConnectError::Protocol(_))
        ));
    }

    #[test]
    fn privileges_accept_numbers_and_booleans() {
        let info = serde_json::json!({ "a": 1, "b": 0, "c": true });
        assert!(flag(&info, "a"));
        assert!(!flag(&info, "b"));
        assert!(flag(&info, "c"));
        assert!(!flag(&info, "missing"));
    }
}
