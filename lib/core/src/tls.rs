//! TLS trust policy and the TLS transport.
//!
//! Some targets — iLO especially — ship self-signed or expired certificates, so
//! connecting insecurely has to be possible. It must never be a blanket "skip
//! verification" toggle though: the user accepts a *specific* certificate
//! (trust-on-first-use), and only that certificate is then trusted.
//!
//! [`connect`] opens a TLS stream under a [`TlsPolicy`]. A certificate that
//! fails normal verification is rejected with
//! [`ConnectError::UntrustedCertificate`], carrying its fingerprint, unless the
//! policy names that exact fingerprint as accepted.

use std::fmt;
use std::sync::{Arc, Mutex};

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use crate::{ConnectError, Target};

/// SHA-256 fingerprint of a peer certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CertFingerprint(pub [u8; 32]);

impl CertFingerprint {
    /// The fingerprint of a DER-encoded certificate.
    pub fn of(der: &[u8]) -> Self {
        Self(Sha256::digest(der).into())
    }

    /// Parses the form [`Display`](fmt::Display) produces (colon-separated
    /// hex), also accepting plain hex without separators.
    pub fn parse(text: &str) -> Option<Self> {
        let hex: String = text.chars().filter(|c| *c != ':').collect();
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl fmt::Display for CertFingerprint {
    /// Upper-case hex pairs separated by colons, as browsers show them.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(":")?;
            }
            write!(f, "{byte:02X}")?;
        }
        Ok(())
    }
}

/// How strictly a peer's TLS certificate is validated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum TlsPolicy {
    /// Require a valid certificate chain. The default.
    #[default]
    Secure,
    /// Accept an otherwise-invalid certificate, but only if it matches a
    /// fingerprint the user has already approved.
    ///
    /// `accepted` is `None` when the user has not approved one yet: the
    /// connector should surface the peer's fingerprint via
    /// [`ConnectError::UntrustedCertificate`](crate::ConnectError::UntrustedCertificate)
    /// so the UI can prompt, rather than connecting blindly.
    AllowInsecure { accepted: Option<CertFingerprint> },
}

impl TlsPolicy {
    /// The fingerprint this policy trusts beyond the normal checks, if any.
    pub fn accepted(&self) -> Option<CertFingerprint> {
        match self {
            TlsPolicy::Secure => None,
            TlsPolicy::AllowInsecure { accepted } => *accepted,
        }
    }
}

/// Opens a TCP connection to `target` and negotiates TLS over it under
/// `policy`.
///
/// Any certificate the system would reject — self-signed, expired, wrong
/// name — fails with [`ConnectError::UntrustedCertificate`] unless `policy`
/// accepts that exact fingerprint. The peer still has to prove it holds the
/// certificate's key either way.
pub async fn connect(
    target: &Target,
    policy: &TlsPolicy,
) -> Result<TlsStream<TcpStream>, ConnectError> {
    let tcp = TcpStream::connect((target.host.as_str(), target.port)).await?;
    tcp.set_nodelay(true)?;
    let server_name = ServerName::try_from(target.host.clone())
        .map_err(|error| ConnectError::Tls(error.to_string()))?;

    let verifier = Arc::new(TofuVerifier::new(policy.accepted())?);
    let config = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|error| ConnectError::Tls(error.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();

    match TlsConnector::from(Arc::new(config))
        .connect(server_name, tcp)
        .await
    {
        Ok(stream) => Ok(stream),
        Err(error) => match verifier.rejected() {
            Some(fingerprint) => Err(ConnectError::UntrustedCertificate(fingerprint)),
            None => Err(ConnectError::Tls(error.to_string())),
        },
    }
}

/// The crypto provider every TLS connection uses. Named explicitly, because
/// the dependency tree enables more than one and rustls won't pick.
fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

/// Verifies certificates normally, but accepts one specific certificate the
/// user approved, and remembers the fingerprint of any it rejects so
/// [`connect`] can report it.
#[derive(Debug)]
struct TofuVerifier {
    webpki: Arc<WebPkiServerVerifier>,
    accepted: Option<CertFingerprint>,
    rejected: Mutex<Option<CertFingerprint>>,
}

impl TofuVerifier {
    fn new(accepted: Option<CertFingerprint>) -> Result<Self, ConnectError> {
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let webpki = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider())
            .build()
            .map_err(|error| ConnectError::Tls(error.to_string()))?;
        Ok(Self {
            webpki,
            accepted,
            rejected: Mutex::new(None),
        })
    }

    fn rejected(&self) -> Option<CertFingerprint> {
        *self.rejected.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fingerprint = CertFingerprint::of(end_entity);
        if self.accepted == Some(fingerprint) {
            return Ok(ServerCertVerified::assertion());
        }
        let result = self.webpki.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );
        if result.is_err() {
            *self.rejected.lock().unwrap_or_else(|e| e.into_inner()) = Some(fingerprint);
        }
        result
    }

    // Signatures are always checked, so an accepted fingerprint still
    // requires the peer to hold the certificate's private key.
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.webpki.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.webpki.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.webpki.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::ServerConfig;
    use rustls::pki_types::PrivateKeyDer;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn fingerprint_round_trips_through_display() {
        let fingerprint = CertFingerprint::of(b"certificate");
        let shown = fingerprint.to_string();
        assert_eq!(shown.len(), 32 * 3 - 1);
        assert_eq!(CertFingerprint::parse(&shown), Some(fingerprint));
        assert_eq!(
            CertFingerprint::parse(&shown.replace(':', "")),
            Some(fingerprint)
        );
        assert_eq!(CertFingerprint::parse("AB:CD"), None);
    }

    /// Serves one TLS connection with a fresh self-signed certificate, echoing
    /// a byte back. Returns the port and the certificate's fingerprint.
    async fn self_signed_server() -> (u16, CertFingerprint) {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
        let der = cert.cert.der().clone();
        let fingerprint = CertFingerprint::of(&der);
        let key = PrivateKeyDer::Pkcs8(cert.signing_key.serialize_der().into());
        let config = ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![der], key)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(tcp).await {
                        let mut byte = [0];
                        if tls.read_exact(&mut byte).await.is_ok() {
                            let _ = tls.write_all(&byte).await;
                        }
                    }
                });
            }
        });
        (port, fingerprint)
    }

    #[tokio::test]
    async fn untrusted_certificate_reports_its_fingerprint() {
        let (port, fingerprint) = self_signed_server().await;
        let target = Target::new("localhost", port);

        for policy in [
            TlsPolicy::Secure,
            TlsPolicy::AllowInsecure { accepted: None },
            TlsPolicy::AllowInsecure {
                accepted: Some(CertFingerprint([0; 32])),
            },
        ] {
            match connect(&target, &policy).await {
                Err(ConnectError::UntrustedCertificate(seen)) => assert_eq!(seen, fingerprint),
                other => panic!("expected an untrusted certificate, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn accepted_fingerprint_connects() {
        let (port, fingerprint) = self_signed_server().await;
        let policy = TlsPolicy::AllowInsecure {
            accepted: Some(fingerprint),
        };

        let mut stream = connect(&Target::new("localhost", port), &policy)
            .await
            .unwrap();
        stream.write_all(b"x").await.unwrap();
        let mut byte = [0];
        stream.read_exact(&mut byte).await.unwrap();
        assert_eq!(&byte, b"x");
    }
}
