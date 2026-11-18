//! End-to-end tests against a mock iLO: an HTTPS server with a self-signed
//! certificate for the web API, and a console port that negotiates, sends a
//! session header and a little video, and reports power and health, all
//! with fixed test keys. Modelled on what a real iLO 4 (firmware 2.82) did.

use std::sync::{Arc, Mutex};

use rustls::ServerConfig;
use rustls::pki_types::PrivateKeyDer;
use server_remote_core::{CertFingerprint, PointerButton};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::time::timeout;

use super::*;
use crate::command::tests::packet;
use crate::dvc::tests::{BitWriter, HEADER, RESYNC};
use crate::web::Features;

const SESSION_KEY: &str = "0123456789abcdef0123456789abcdef";
const ENC_KEY: &str = "00112233445566778899aabbccddeeff";
const CMD_ENC_KEY: &str = "ffeeddccbbaa99887766554433221100";
const PASSWORD: &str = "secret";
const FEATURES: Features = Features {
    encrypt_key: true,
    encrypt_vm_key: true,
    encrypt_cmd: true,
};
const WAIT: Duration = Duration::from_secs(10);

fn key(text: &str) -> [u8; 16] {
    core::array::from_fn(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap())
}

struct Mock {
    web: Target,
    fingerprint: CertFingerprint,
    /// Web API calls, as "METHOD path [method]".
    calls: Arc<Mutex<Vec<String>>>,
    /// What the client sent on the KVM connection, decrypted.
    kvm_sent: mpsc::Receiver<Vec<u8>>,
}

async fn mock() -> Mock {
    let console = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rc_port = console.local_addr().unwrap().port();
    let (web, fingerprint, calls) = web_server(rc_port).await;
    let (kvm_tx, kvm_sent) = mpsc::channel(64);
    tokio::spawn(async move {
        let (kvm, _) = console.accept().await.unwrap();
        tokio::spawn(serve_kvm(kvm, kvm_tx));
        let (command, _) = console.accept().await.unwrap();
        tokio::spawn(serve_command(command));
    });
    Mock {
        web,
        fingerprint,
        calls,
        kvm_sent,
    }
}

impl Mock {
    fn policy(&self) -> TlsPolicy {
        TlsPolicy::AllowInsecure {
            accepted: Some(self.fingerprint),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

async fn web_server(rc_port: u16) -> (Target, CertFingerprint, Arc<Mutex<Vec<String>>>) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let der = cert.cert.der().clone();
    let fingerprint = CertFingerprint::of(&der);
    let key = PrivateKeyDer::Pkcs8(cert.signing_key.serialize_der().into());
    let config = ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![der], key)
    .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let calls = Arc::new(Mutex::new(Vec::new()));

    let log = calls.clone();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let log = log.clone();
            tokio::spawn(async move {
                if let Ok(tls) = acceptor.accept(tcp).await {
                    serve_http(tls, rc_port, &log).await;
                }
            });
        }
    });
    (Target::new("localhost", port), fingerprint, calls)
}

async fn serve_http(
    mut stream: impl AsyncRead + AsyncWrite + Unpin,
    rc_port: u16,
    log: &Mutex<Vec<String>>,
) {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let (head, body) = loop {
        let n = stream.read(&mut buf).await.unwrap();
        raw.extend_from_slice(&buf[..n]);
        if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..end]).into_owned();
            let length: usize = head
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .map_or(0, |n| n.parse().unwrap());
            while raw.len() < end + 4 + length {
                let n = stream.read(&mut buf).await.unwrap();
                raw.extend_from_slice(&buf[..n]);
            }
            break (head, raw[end + 4..end + 4 + length].to_vec());
        }
    };
    let mut words = head.split_whitespace();
    let (method, path) = (words.next().unwrap(), words.next().unwrap());
    let has_cookie = head.contains(&format!("Cookie: sessionKey={SESSION_KEY}"));
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let call = json["method"].as_str().unwrap_or_default();
    log.lock()
        .unwrap()
        .push(format!("{method} {path} {call}").trim().to_owned());

    let (status, cookie, reply) = match (path, call) {
        ("/json/login_session", "login") if json["password"] == PASSWORD => (
            "200 OK",
            true,
            serde_json::json!({
                "session_key": SESSION_KEY, "user_name": "test",
                "login_priv": 1, "remote_cons_priv": 1, "virtual_media_priv": 1, "reset_priv": 1,
            }),
        ),
        ("/json/login_session", "login") => (
            "403 Forbidden",
            false,
            serde_json::json!({"message": "JS_ERR_NO_PRIV", "details": "Invalid login attempt"}),
        ),
        ("/json/login_session", "logout") if has_cookie => ("200 OK", false, serde_json::json!({})),
        ("/json/rc_info", _) if has_cookie => (
            "200 OK",
            false,
            serde_json::json!({
                "enc_key": ENC_KEY, "enc_type": 0, "rc_port": rc_port, "vm_key": ENC_KEY,
                "vm_port": 17988, "cmd_enc_key": CMD_ENC_KEY, "protocol_version": "1.1",
                "optional_features": "ENCRYPT_KEY;ENCRYPT_VMKEY;ENCRYPT_CMD",
                "server_name": "", "ilo_fqdn": "ilo.test", "blade": 0, "bay": null,
            }),
        ),
        _ => ("500 Internal Server Error", false, Value::Null),
    };
    let body = if reply.is_null() {
        Vec::new()
    } else {
        serde_json::to_vec(&reply).unwrap()
    };
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/x-javascript\r\nContent-Length: {}\r\n",
        body.len()
    );
    if cookie {
        response.push_str(&format!(
            "Set-Cookie: sessionKey={SESSION_KEY}; path=/; secure\r\n"
        ));
    }
    response.push_str("\r\n");
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.write_all(&body).await;
    let _ = stream.shutdown().await;
}

/// Checks the greeting and key request, then grants the console.
async fn grant(stream: &mut TcpStream, channel: Channel) {
    stream.write_all(b"P").await.unwrap();
    let mut request = [0u8; 34];
    stream.read_exact(&mut request).await.unwrap();
    let expected = negotiate::key_request(channel, SESSION_KEY, ENC_KEY, FEATURES);
    let reply = if request == expected { b"R" } else { b"Q" };
    stream.write_all(reply).await.unwrap();
}

/// A 2×1-block screen: red on the left, blue on the right.
async fn serve_kvm(mut stream: TcpStream, sent: mpsc::Sender<Vec<u8>>) {
    grant(&mut stream, Channel::Kvm).await;

    let mut video = BitWriter::default();
    video
        .mode(2, 1)
        .solid_block(0, 0, (31, 0, 0))
        .solid_block(1, 0, (0, 0, 31))
        .frame();
    let mut body = RESYNC.to_vec();
    body.extend(video.bytes());
    Keystream::new(CipherKind::Rc4, &key(ENC_KEY)).apply(&mut body);
    stream.write_all(&HEADER).await.unwrap();
    stream.write_all(&body).await.unwrap();

    // Everything the client sends is encrypted from the start of the
    // keystream the header chose.
    let mut cipher = Keystream::new(CipherKind::Rc4, &key(ENC_KEY));
    let mut buf = [0u8; 256];
    loop {
        match stream.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                let mut bytes = buf[..n].to_vec();
                cipher.apply(&mut bytes);
                if sent.send(bytes).await.is_err() {
                    return;
                }
            }
        }
    }
}

async fn serve_command(mut stream: TcpStream) {
    grant(&mut stream, Channel::Command).await;
    let mut start = [0u8; 4];
    stream.read_exact(&mut start).await.unwrap();
    assert_eq!(start, command::START_ENCRYPTION);

    let mut wire = packet(3, &[1]);
    wire.extend(packet(4, &[0]));
    Keystream::new(CipherKind::Aes128, &key(CMD_ENC_KEY)).apply(&mut wire);
    stream.write_all(&wire).await.unwrap();
    // Hold the connection open until the client goes.
    let _ = stream.read(&mut [0u8; 64]).await;
}

async fn next_event(handle: &mut SessionHandle) -> SessionEvent {
    timeout(WAIT, handle.events.recv())
        .await
        .expect("no session event in time")
        .expect("the session ended")
}

/// Reads client KVM bytes until `count` have arrived.
async fn kvm_bytes(mock: &mut Mock, count: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    while bytes.len() < count {
        let chunk = timeout(WAIT, mock.kvm_sent.recv())
            .await
            .expect("the client sent nothing in time")
            .expect("the KVM connection closed");
        bytes.extend(chunk);
    }
    bytes
}

#[tokio::test]
async fn a_session_shows_video_reports_status_and_sends_input() {
    let mut mock = mock().await;
    let mut handle = IloConnector::new("test".into(), PASSWORD.into())
        .connect(&mock.web, &mock.policy())
        .await
        .unwrap();

    match next_event(&mut handle).await {
        SessionEvent::Connected { capabilities, .. } => {
            assert_eq!(capabilities.pointer, PointerMode::Absolute);
            assert!(
                capabilities
                    .input_actions
                    .iter()
                    .any(|a| a.id == CAD_ACTION)
            );
            assert!(matches!(
                &capabilities.toolbar[0],
                ToolbarItem::Menu { id, .. } if id == "ilo.power"
            ));
        }
        other => panic!("expected Connected, got {other:?}"),
    }

    let mut frame = None;
    let mut powered = false;
    let mut resized = false;
    while frame.is_none() || !powered {
        match next_event(&mut handle).await {
            SessionEvent::Resized { width, height } => {
                assert_eq!((width, height), (32, 16));
                resized = true;
            }
            SessionEvent::Frame(f) => frame = Some(f),
            SessionEvent::Status(status) => {
                powered |= status.items.iter().any(|item| {
                    matches!(item, StatusItem::Indicator { label, state: IndicatorState::Ok }
                        if label == "Powered on")
                });
                assert_eq!(status.encryption.as_deref(), Some("RC4"));
            }
            SessionEvent::Capabilities(_) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(resized);
    let frame = frame.unwrap();
    assert_eq!((frame.width, frame.height), (32, 16));
    assert_eq!(&frame.pixels[..4], &[255, 0, 0, 255]);
    assert_eq!(&frame.pixels[16 * 4..16 * 4 + 4], &[0, 0, 255, 255]);

    // After the header: all keys up, then a refresh.
    let opening = kvm_bytes(&mut mock, 12).await;
    assert_eq!(&opening[..10], &input::report(0, &[]));
    assert_eq!(&opening[10..12], &REFRESH);

    handle
        .input
        .send(InputEvent::Key {
            keysym: 0x61,
            pressed: true,
        })
        .await
        .unwrap();
    assert_eq!(kvm_bytes(&mut mock, 10).await, input::report(0, &[0x04]));

    handle
        .input
        .send(InputEvent::PointerMoved { x: 16.0, y: 8.0 })
        .await
        .unwrap();
    handle
        .input
        .send(InputEvent::PointerButton {
            button: PointerButton::Left,
            pressed: true,
        })
        .await
        .unwrap();
    let mouse = kvm_bytes(&mut mock, 20).await;
    assert_eq!(&mouse[..2], &[0x02, 0x00]);
    assert_eq!(u16::from_le_bytes([mouse[2], mouse[3]]), 1500);
    assert_eq!(u16::from_le_bytes([mouse[4], mouse[5]]), 1500);
    assert_eq!(mouse[18], 0x01);

    // Closing the session says goodbye on the KVM connection and logs out.
    drop(handle);
    assert_eq!(kvm_bytes(&mut mock, 2).await, CLOSING);
    timeout(WAIT, async {
        while !mock
            .calls()
            .contains(&"POST /json/login_session logout".to_owned())
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("no logout");
    assert_eq!(
        mock.calls()[..2],
        [
            "POST /json/login_session login".to_owned(),
            "GET /json/rc_info".to_owned()
        ]
    );
}

#[tokio::test]
async fn an_unapproved_certificate_reports_its_fingerprint() {
    let mock = mock().await;
    let result = IloConnector::new("test".into(), PASSWORD.into())
        .connect(&mock.web, &TlsPolicy::AllowInsecure { accepted: None })
        .await;
    match result {
        Err(ConnectError::UntrustedCertificate(seen)) => assert_eq!(seen, mock.fingerprint),
        other => panic!("expected an untrusted certificate, got {:?}", other.err()),
    }
}

#[tokio::test]
async fn a_wrong_password_is_an_auth_error() {
    let mock = mock().await;
    let result = IloConnector::new("test".into(), "wrong".into())
        .connect(&mock.web, &mock.policy())
        .await;
    assert!(matches!(result, Err(ConnectError::Auth(_))));
}

#[tokio::test]
async fn a_missing_user_name_fails_before_connecting() {
    let result = IloConnector::new(String::new(), PASSWORD.into())
        .connect(&Target::new("127.0.0.1", 1), &TlsPolicy::Secure)
        .await;
    assert!(matches!(result, Err(ConnectError::Auth(_))));
}

#[test]
fn power_actions_follow_the_power_state() {
    let enabled = |power| match power_menu(power) {
        ToolbarItem::Menu { items, .. } => items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action(action) => Some((action.id.to_string(), action.enabled)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => unreachable!(),
    };
    assert!(enabled(Some(true)).iter().all(|(_, on)| *on));
    let off = enabled(Some(false));
    assert!(off.contains(&(POWER_PRESS_ACTION.to_owned(), true)));
    assert!(off.contains(&(POWER_RESET_ACTION.to_owned(), false)));
}

#[test]
fn web_url_brackets_ipv6() {
    assert_eq!(web_url(&Target::new("ilo", 443)), "https://ilo/");
    assert_eq!(web_url(&Target::new("::1", 8443)), "https://[::1]:8443/");
}

use serde_json::Value;
