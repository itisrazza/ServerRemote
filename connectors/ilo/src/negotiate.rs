//! The v1 negotiation that opens the KVM and command connections. See
//! "v1 negotiation" in `docs/protocols/ilo4.md`. Everything here is
//! plaintext.

use std::time::Duration;

use server_remote_core::ConnectError;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::web::{Features, RcInfo};

/// Which connection is being negotiated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Kvm,
    Command,
}

impl Channel {
    fn request(self) -> u16 {
        match self {
            Channel::Kvm => 0x2001,
            Channel::Command => 0x2002,
        }
    }
}

/// The greeting the iLO opens each connection with.
const GREETING: u8 = b'P';
/// How many bytes the command connection skips looking for the greeting.
const GREETING_SEARCH: usize = 30;
/// How long to wait for each byte of the negotiation.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Whether the remote console is licensed, from a successful negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granted {
    Licensed,
    Unlicensed,
}

/// Runs the negotiation on a fresh connection.
pub async fn negotiate(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    channel: Channel,
    session_key: &str,
    rc: &RcInfo,
) -> Result<Granted, ConnectError> {
    let tries = match channel {
        Channel::Kvm => 1,
        Channel::Command => GREETING_SEARCH,
    };
    let mut greeted = false;
    for _ in 0..tries {
        if read_byte(stream).await? == GREETING {
            greeted = true;
            break;
        }
    }
    if !greeted {
        return Err(ConnectError::Protocol(
            "the iLO didn't greet the console connection".into(),
        ));
    }

    stream
        .write_all(&key_request(
            channel,
            session_key,
            &rc.enc_key_text,
            rc.features,
        ))
        .await?;
    stream.flush().await?;

    match read_byte(stream).await? {
        b'R' => Ok(Granted::Licensed),
        b'W' => Ok(Granted::Unlicensed),
        b'S' | b'Y' => Err(ConnectError::Protocol(
            "another user is using the remote console".into(),
        )),
        b'Q' => Err(ConnectError::Auth(
            "the iLO refused the console session (session limit reached or invalid key)".into(),
        )),
        b'X' => Err(ConnectError::Protocol(
            "the iLO has no free console sessions".into(),
        )),
        // The vendor client treats anything else as success.
        _ => Ok(Granted::Licensed),
    }
}

/// `NEG_REQ_KEY`: the request type, then the session key, zero-padded to 32
/// bytes and, with `ENCRYPT_KEY`, XORed with the hex text of `enc_key`.
pub fn key_request(channel: Channel, session_key: &str, pad: &str, features: Features) -> [u8; 34] {
    let mut cmd = channel.request();
    let mut key = [0u8; 32];
    for (slot, byte) in key.iter_mut().zip(session_key.bytes()) {
        *slot = byte;
    }
    if features.encrypt_key && !pad.is_empty() {
        let len = session_key.len().min(32);
        for (i, byte) in key[..len].iter_mut().enumerate() {
            *byte ^= pad.as_bytes()[i % pad.len()];
        }
        cmd |= if features.encrypt_vm_key {
            0x4000
        } else {
            0x8000
        };
    }
    let mut request = [0u8; 34];
    request[..2].copy_from_slice(&cmd.to_le_bytes());
    request[2..].copy_from_slice(&key);
    request
}

async fn read_byte(stream: &mut (impl AsyncRead + Unpin)) -> Result<u8, ConnectError> {
    match tokio::time::timeout(TIMEOUT, stream.read_u8()).await {
        Ok(Ok(byte)) => Ok(byte),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => Err(
            ConnectError::Protocol("the iLO closed the console connection".into()),
        ),
        Ok(Err(error)) => Err(error.into()),
        Err(_) => Err(ConnectError::Protocol(
            "the iLO stopped answering the console negotiation".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features(encrypt_key: bool, encrypt_vm_key: bool) -> Features {
        Features {
            encrypt_key,
            encrypt_vm_key,
            encrypt_cmd: true,
        }
    }

    #[test]
    fn matches_a_request_the_ilo_accepted() {
        // From a capture against firmware 2.82, whose keys are long dead.
        let request = key_request(
            Channel::Kvm,
            "dc27aa8b33fff1843563eb5ef39a4e22",
            "99bf80275ba7b7911e271f6c76ab9e6b",
            features(true, true),
        );
        let expected: Vec<u8> = (0..34)
            .map(|i| {
                u8::from_str_radix(
                    &"01605d5a505159510a5506510751040601050250040454040306510558030d000450"
                        [i * 2..i * 2 + 2],
                    16,
                )
                .unwrap()
            })
            .collect();
        assert_eq!(request.to_vec(), expected);
    }

    #[test]
    fn plain_requests_carry_the_key_as_is() {
        let request = key_request(Channel::Command, "abc", "00", features(false, false));
        assert_eq!(&request[..2], &[0x02, 0x20]);
        assert_eq!(&request[2..5], b"abc");
        assert!(request[5..].iter().all(|&byte| byte == 0));
    }

    #[test]
    fn obfuscation_xors_only_the_key_and_round_trips() {
        let pad = "0123456789abcdef0123456789abcdef";
        let request = key_request(Channel::Kvm, "key", pad, features(true, false));
        assert_eq!(u16::from_le_bytes([request[0], request[1]]), 0xA001);
        let unmasked: Vec<u8> = request[2..5]
            .iter()
            .zip(pad.bytes())
            .map(|(a, b)| a ^ b)
            .collect();
        assert_eq!(unmasked, b"key");
        assert!(request[5..].iter().all(|&byte| byte == 0));
    }

    #[tokio::test]
    async fn statuses_map_to_results() {
        let rc = RcInfo {
            enc_key: [0; 16],
            enc_key_text: "00".repeat(16),
            cmd_enc_key: [0; 16],
            rc_port: 0,
            features: Features::default(),
            server_name: None,
        };
        for (reply, ok) in [(b'R', true), (b'W', true), (b'S', false), (b'Q', false)] {
            let (mut client, mut server) = tokio::io::duplex(64);
            server.write_all(&[GREETING, reply]).await.unwrap();
            let result = negotiate(&mut client, Channel::Kvm, "k", &rc).await;
            assert_eq!(result.is_ok(), ok, "reply {}", reply as char);
            let mut sent = [0u8; 34];
            server.read_exact(&mut sent).await.unwrap();
            assert_eq!(&sent[..2], &[0x01, 0x20]);
        }
    }

    #[tokio::test]
    async fn the_command_channel_skips_noise_before_the_greeting() {
        let rc = RcInfo {
            enc_key: [0; 16],
            enc_key_text: String::new(),
            cmd_enc_key: [0; 16],
            rc_port: 0,
            features: Features::default(),
            server_name: None,
        };
        let (mut client, mut server) = tokio::io::duplex(64);
        server.write_all(&[0, 1, 2, GREETING, b'R']).await.unwrap();
        assert!(
            negotiate(&mut client, Channel::Command, "k", &rc)
                .await
                .is_ok()
        );
    }
}
