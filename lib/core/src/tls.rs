//! TLS trust policy and the TLS transport.
//!
//! Some targets — iLO especially — ship self-signed or expired certificates, so
//! connecting insecurely has to be possible. Normally the user accepts a
//! *specific* certificate (trust-on-first-use), and only that certificate is
//! then trusted; accepting any certificate is a separate, explicit policy.
//!
//! [`connect`] opens a TLS stream under a [`TlsPolicy`]. A certificate that
//! fails normal verification is rejected with
//! [`ConnectError::UntrustedCertificate`], carrying its fingerprint, unless the
//! policy accepts it.
//!
//! An accepted certificate (or any, under [`TlsPolicy::AcceptAny`]) may also
//! use a legacy RSA key (1024 bits and up,
//! with SHA-1 signatures), as older iLO firmware does. The peer still has to
//! prove it holds that key; strict verification is unchanged.

use std::fmt;
use std::sync::{Arc, Mutex};

use aws_lc_rs::signature::{
    RSA_PKCS1_1024_8192_SHA1_FOR_LEGACY_USE_ONLY, RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY,
    RSA_PKCS1_1024_8192_SHA512_FOR_LEGACY_USE_ONLY, RsaParameters, UnparsedPublicKey,
};
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{
    AlgorithmIdentifier, CertificateDer, InvalidSignature, ServerName,
    SignatureVerificationAlgorithm, UnixTime, alg_id,
};
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
    /// Accept any certificate without asking. The connection is still
    /// encrypted to the key the peer presents, but nothing checks whose key
    /// that is, so anyone on the network path can impersonate the peer.
    AcceptAny,
}

impl TlsPolicy {
    /// Whether this policy trusts `cert` without the normal checks.
    fn accepts(&self, cert: &CertificateDer<'_>) -> bool {
        match self {
            TlsPolicy::Secure => false,
            TlsPolicy::AllowInsecure { accepted } => *accepted == Some(CertFingerprint::of(cert)),
            TlsPolicy::AcceptAny => true,
        }
    }

    /// Whether this policy can trust some certificate the normal checks
    /// reject.
    fn trusts_beyond_roots(&self) -> bool {
        matches!(
            self,
            TlsPolicy::AllowInsecure { accepted: Some(_) } | TlsPolicy::AcceptAny
        )
    }
}

/// Opens a TCP connection to `target` and negotiates TLS over it under
/// `policy`.
///
/// Any certificate the system would reject — self-signed, expired, wrong
/// name — fails with [`ConnectError::UntrustedCertificate`] unless `policy`
/// accepts it. The peer still has to prove it holds the certificate's key
/// either way.
pub async fn connect(
    target: &Target,
    policy: &TlsPolicy,
) -> Result<TlsStream<TcpStream>, ConnectError> {
    let tcp = TcpStream::connect((target.host.as_str(), target.port)).await?;
    tcp.set_nodelay(true)?;
    let server_name = ServerName::try_from(target.host.clone())
        .map_err(|error| ConnectError::Tls(error.to_string()))?;

    let verifier = Arc::new(TofuVerifier::new(policy.clone())?);
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

/// Verifies certificates normally, except those the policy accepts, and
/// remembers the fingerprint of any it rejects so [`connect`] can report it.
#[derive(Debug)]
struct TofuVerifier {
    webpki: Arc<WebPkiServerVerifier>,
    policy: TlsPolicy,
    rejected: Mutex<Option<CertFingerprint>>,
}

impl TofuVerifier {
    fn new(policy: TlsPolicy) -> Result<Self, ConnectError> {
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let webpki = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider())
            .build()
            .map_err(|error| ConnectError::Tls(error.to_string()))?;
        Ok(Self {
            webpki,
            policy,
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
        if self.policy.accepts(end_entity) {
            return Ok(ServerCertVerified::assertion());
        }
        let fingerprint = CertFingerprint::of(end_entity);
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
    // requires the peer to hold the certificate's private key. Only the key
    // size and digest rules relax, and only for that certificate.
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        let is_legacy_scheme = LEGACY_RSA
            .mapping
            .iter()
            .any(|(scheme, _)| *scheme == dss.scheme);
        if is_legacy_scheme && self.policy.accepts(cert) {
            return rustls::crypto::verify_tls12_signature(message, cert, dss, &LEGACY_RSA);
        }
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
        let schemes = self.webpki.supported_verify_schemes();
        if !self.policy.trusts_beyond_roots() {
            return schemes;
        }
        let legacy_only: Vec<SignatureScheme> = LEGACY_RSA
            .supported_schemes()
            .into_iter()
            .filter(|scheme| !schemes.contains(scheme))
            .collect();
        [schemes, legacy_only].concat()
    }
}

/// RSA PKCS#1 verification that accepts keys from 1024 bits, which older
/// management controllers (iLO 3, early iLO 4) still ship with.
#[derive(Debug)]
struct LegacyRsa {
    signature_alg_id: AlgorithmIdentifier,
    parameters: &'static RsaParameters,
}

impl SignatureVerificationAlgorithm for LegacyRsa {
    fn verify_signature(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), InvalidSignature> {
        UnparsedPublicKey::new(self.parameters, public_key)
            .verify(message, signature)
            .map_err(|_| InvalidSignature)
    }

    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::RSA_ENCRYPTION
    }

    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        self.signature_alg_id
    }
}

/// `sha1WithRSAEncryption` (1.2.840.113549.1.1.5) with NULL parameters, which
/// `alg_id` doesn't provide.
const RSA_PKCS1_SHA1: AlgorithmIdentifier = AlgorithmIdentifier::from_slice(&[
    0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x05, 0x05, 0x00,
]);

static LEGACY_RSA_SHA1: LegacyRsa = LegacyRsa {
    signature_alg_id: RSA_PKCS1_SHA1,
    parameters: &RSA_PKCS1_1024_8192_SHA1_FOR_LEGACY_USE_ONLY,
};
static LEGACY_RSA_SHA256: LegacyRsa = LegacyRsa {
    signature_alg_id: alg_id::RSA_PKCS1_SHA256,
    parameters: &RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY,
};
static LEGACY_RSA_SHA512: LegacyRsa = LegacyRsa {
    signature_alg_id: alg_id::RSA_PKCS1_SHA512,
    parameters: &RSA_PKCS1_1024_8192_SHA512_FOR_LEGACY_USE_ONLY,
};

/// TLS 1.2 handshake signatures allowed for an accepted certificate, weakest
/// preferred last. aws-lc-rs has no legacy SHA-384 variant.
static LEGACY_RSA: WebPkiSupportedAlgorithms = WebPkiSupportedAlgorithms {
    all: &[&LEGACY_RSA_SHA512, &LEGACY_RSA_SHA256, &LEGACY_RSA_SHA1],
    mapping: &[
        (SignatureScheme::RSA_PKCS1_SHA512, &[&LEGACY_RSA_SHA512]),
        (SignatureScheme::RSA_PKCS1_SHA256, &[&LEGACY_RSA_SHA256]),
        (SignatureScheme::RSA_PKCS1_SHA1, &[&LEGACY_RSA_SHA1]),
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::ServerConfig;
    use rustls::internal::msgs::codec::Codec;
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
    async fn accepting_policies_connect() {
        let (port, fingerprint) = self_signed_server().await;
        for policy in [
            TlsPolicy::AllowInsecure {
                accepted: Some(fingerprint),
            },
            TlsPolicy::AcceptAny,
        ] {
            let mut stream = connect(&Target::new("localhost", port), &policy)
                .await
                .unwrap();
            stream.write_all(b"x").await.unwrap();
            let mut byte = [0];
            stream.read_exact(&mut byte).await.unwrap();
            assert_eq!(&byte, b"x", "{policy:?}");
        }
    }

    /// rustls only builds these internally, so decode one from its wire form.
    fn signed(scheme: SignatureScheme, signature: &[u8]) -> DigitallySignedStruct {
        let length = u16::try_from(signature.len()).unwrap();
        let wire = [
            &u16::from(scheme).to_be_bytes()[..],
            &length.to_be_bytes(),
            signature,
        ]
        .concat();
        DigitallySignedStruct::read_bytes(&wire).unwrap()
    }

    // aws-lc-rs refuses to sign with a 1024-bit key, so these fixtures come
    // from `testdata/legacy-rsa/generate.sh` instead of a live handshake.
    #[test]
    fn legacy_rsa_signatures_verify_only_for_an_accepted_certificate() {
        let cert = CertificateDer::from(&include_bytes!("../testdata/legacy-rsa/cert.der")[..]);
        let message = &include_bytes!("../testdata/legacy-rsa/message.txt")[..];
        let sha1 = &include_bytes!("../testdata/legacy-rsa/signature-sha1.bin")[..];
        let sha512 = &include_bytes!("../testdata/legacy-rsa/signature-sha512.bin")[..];
        let fingerprint = CertFingerprint::of(&cert);

        struct Case<'a> {
            name: &'a str,
            policy: TlsPolicy,
            scheme: SignatureScheme,
            signature: &'a [u8],
            message: &'a [u8],
            valid: bool,
        }
        let cases = [
            Case {
                name: "accepted, SHA-512",
                policy: TlsPolicy::AllowInsecure {
                    accepted: Some(fingerprint),
                },
                scheme: SignatureScheme::RSA_PKCS1_SHA512,
                signature: sha512,
                message,
                valid: true,
            },
            Case {
                name: "accepted, SHA-1",
                policy: TlsPolicy::AllowInsecure {
                    accepted: Some(fingerprint),
                },
                scheme: SignatureScheme::RSA_PKCS1_SHA1,
                signature: sha1,
                message,
                valid: true,
            },
            Case {
                name: "any accepted",
                policy: TlsPolicy::AcceptAny,
                scheme: SignatureScheme::RSA_PKCS1_SHA512,
                signature: sha512,
                message,
                valid: true,
            },
            Case {
                name: "strict",
                policy: TlsPolicy::Secure,
                scheme: SignatureScheme::RSA_PKCS1_SHA512,
                signature: sha512,
                message,
                valid: false,
            },
            Case {
                name: "nothing accepted",
                policy: TlsPolicy::AllowInsecure { accepted: None },
                scheme: SignatureScheme::RSA_PKCS1_SHA512,
                signature: sha512,
                message,
                valid: false,
            },
            Case {
                name: "another certificate accepted",
                policy: TlsPolicy::AllowInsecure {
                    accepted: Some(CertFingerprint([0; 32])),
                },
                scheme: SignatureScheme::RSA_PKCS1_SHA512,
                signature: sha512,
                message,
                valid: false,
            },
            Case {
                name: "tampered message",
                policy: TlsPolicy::AllowInsecure {
                    accepted: Some(fingerprint),
                },
                scheme: SignatureScheme::RSA_PKCS1_SHA512,
                signature: sha512,
                message: b"tampered",
                valid: false,
            },
            Case {
                name: "mismatched digest",
                policy: TlsPolicy::AllowInsecure {
                    accepted: Some(fingerprint),
                },
                scheme: SignatureScheme::RSA_PKCS1_SHA256,
                signature: sha512,
                message,
                valid: false,
            },
        ];

        for case in cases {
            let verifier = TofuVerifier::new(case.policy).unwrap();
            let dss = signed(case.scheme, case.signature);
            let result = verifier.verify_tls12_signature(case.message, &cert, &dss);
            assert_eq!(result.is_ok(), case.valid, "{}: {result:?}", case.name);
        }
    }

    #[test]
    fn sha1_is_advertised_only_with_an_accepted_certificate() {
        let advertises_sha1 = |accepted| {
            TofuVerifier::new(TlsPolicy::AllowInsecure { accepted })
                .unwrap()
                .supported_verify_schemes()
                .contains(&SignatureScheme::RSA_PKCS1_SHA1)
        };
        assert!(!advertises_sha1(None));
        assert!(advertises_sha1(Some(CertFingerprint([0; 32]))));
        let any = TofuVerifier::new(TlsPolicy::AcceptAny).unwrap();
        assert!(
            any.supported_verify_schemes()
                .contains(&SignatureScheme::RSA_PKCS1_SHA1)
        );
    }
}
