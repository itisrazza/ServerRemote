//! The command connection: server packets about power, health and other
//! users, and the short commands the client answers with. See section 6 of
//! `docs/protocols/ilo4.md`.

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::cipher::Keystream;

/// The largest packet body the vendor client accepts.
const MAX_SIZE: usize = 1500;

/// Answers to another user's seize or share request.
pub const NO: [u8; 4] = [3, 0, 0, 0];
pub const YES: [u8; 4] = [4, 0, 0, 0];
/// Switches the channel to AES (`ENCRYPT_CMD`). Sent in plaintext.
pub const START_ENCRYPTION: [u8; 4] = [8, 0, 0, 0];

/// A server packet, decrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub cmd: u32,
    pub seq: u16,
    pub flags: u16,
    pub data: Vec<u8>,
}

/// Reads one whole packet, decrypting it with `cipher`.
pub async fn read_packet(
    stream: &mut (impl AsyncRead + Unpin),
    cipher: &mut Keystream,
) -> std::io::Result<Packet> {
    let mut header = [0u8; 12];
    stream.read_exact(&mut header).await?;
    cipher.apply(&mut header);
    let cmd = u32::from_le_bytes(header[0..4].try_into().unwrap_or_default());
    let size = u32::from_le_bytes(header[4..8].try_into().unwrap_or_default()) as usize;
    if size > MAX_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("command packet of {size} bytes is too large"),
        ));
    }
    let mut data = vec![0u8; size];
    stream.read_exact(&mut data).await?;
    cipher.apply(&mut data);
    Ok(Packet {
        cmd,
        seq: u16::from_le_bytes([header[8], header[9]]),
        flags: u16::from_le_bytes([header[10], header[11]]),
        data,
    })
}

/// The server's health LED.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Normal,
    Caution,
    Critical,
    Inactive,
}

/// What a packet means to the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Power(bool),
    Health(Health),
    PostCode(u16),
    /// Another user is taking the console over.
    Seize {
        user: String,
    },
    /// Another user asks to share the console.
    Share {
        user: String,
    },
    /// The session can't go on: firmware update, reset, licence or rights.
    Ended(&'static str),
    /// Anything the client doesn't act on.
    Ignored,
}

impl Packet {
    pub fn notice(&self) -> Notice {
        let byte = |i: usize| self.data.get(i).copied();
        match self.cmd {
            3 => byte(0).map_or(Notice::Ignored, |on| Notice::Power(on == 1)),
            4 => byte(0).map_or(Notice::Ignored, |led| {
                Notice::Health(match led {
                    0 => Health::Normal,
                    1 => Health::Caution,
                    2 => Health::Critical,
                    _ => Health::Inactive,
                })
            }),
            5 => match (byte(0), byte(1)) {
                (Some(low), Some(high)) => Notice::PostCode(u16::from_le_bytes([low, high])),
                _ => Notice::Ignored,
            },
            6 => Notice::Seize { user: self.user() },
            9 => Notice::Share { user: self.user() },
            10 => Notice::Ended("the iLO is updating its firmware"),
            11 => Notice::Ended("the iLO says this user may not use the remote console"),
            14 => Notice::Ended("the remote console isn't licensed on this iLO"),
            15 => Notice::Ended("the iLO is resetting"),
            _ => Notice::Ignored,
        }
    }

    /// "user (address)" from a seize or share request.
    fn user(&self) -> String {
        let field = |range: std::ops::Range<usize>| {
            let bytes = self.data.get(range).unwrap_or_default();
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
        };
        let (name, address) = (field(0..64), field(64..128));
        match (name.is_empty(), address.is_empty()) {
            (false, false) => format!("{name} ({address})"),
            (false, true) => name,
            (true, false) => address,
            (true, true) => "another user".into(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::cipher::CipherKind;

    pub fn packet(cmd: u32, data: &[u8]) -> Vec<u8> {
        let mut bytes = cmd.to_le_bytes().to_vec();
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend([0, 0, 0, 0]);
        bytes.extend(data);
        bytes
    }

    #[tokio::test]
    async fn reads_encrypted_packets_back_to_back() {
        let key = [5u8; 16];
        let mut wire = packet(3, &[1]);
        wire.extend(packet(4, &[1]));
        Keystream::new(CipherKind::Aes128, &key).apply(&mut wire);

        let mut cipher = Keystream::new(CipherKind::Aes128, &key);
        let mut reader = wire.as_slice();
        let power = read_packet(&mut reader, &mut cipher).await.unwrap();
        let health = read_packet(&mut reader, &mut cipher).await.unwrap();
        assert_eq!(power.notice(), Notice::Power(true));
        assert_eq!(health.notice(), Notice::Health(Health::Caution));
    }

    #[tokio::test]
    async fn oversized_packets_are_rejected() {
        let mut wire = 3u32.to_le_bytes().to_vec();
        wire.extend(4000u32.to_le_bytes());
        wire.extend([0; 4]);
        let mut reader = wire.as_slice();
        assert!(
            read_packet(&mut reader, &mut Keystream::None)
                .await
                .is_err()
        );
    }

    #[test]
    fn post_codes_and_requests_decode() {
        let post = Packet {
            cmd: 5,
            seq: 0,
            flags: 0,
            data: vec![0x00, 0x50],
        };
        assert_eq!(post.notice(), Notice::PostCode(0x5000));

        let mut data = vec![0u8; 128];
        data[..5].copy_from_slice(b"admin");
        data[64..72].copy_from_slice(b"10.0.0.9");
        let seize = Packet {
            cmd: 6,
            seq: 0,
            flags: 30,
            data,
        };
        assert_eq!(
            seize.notice(),
            Notice::Seize {
                user: "admin (10.0.0.9)".into()
            }
        );
    }
}
