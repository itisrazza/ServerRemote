//! The trait every protocol implements to open a session.

use thiserror::Error;

use crate::{CertFingerprint, SessionHandle, Target, TlsPolicy};

/// Why a connection attempt failed.
#[derive(Debug, Error)]
pub enum ConnectError {
    /// The transport could not be established (DNS failure, refused, timed out).
    #[error("transport error: {0}")]
    Transport(#[from] std::io::Error),

    /// The peer presented an untrusted certificate. Carries its fingerprint so
    /// the UI can ask the user whether to accept it (see [`TlsPolicy`]).
    #[error("untrusted certificate")]
    UntrustedCertificate(CertFingerprint),

    /// TLS negotiation failed for a reason other than trust.
    #[error("TLS error: {0}")]
    Tls(String),

    /// Authentication was rejected.
    #[error("authentication failed: {0}")]
    Auth(String),

    /// The protocol handshake failed.
    #[error("protocol error: {0}")]
    Protocol(String),
}

/// A protocol that can open sessions, e.g. VNC or Spice.
///
/// Implementations are cheap, reusable descriptors (usually zero-sized or just
/// holding credentials); the live connection lives behind the returned
/// [`SessionHandle`], not in `self`.
pub trait Connector {
    /// Human-readable protocol name, e.g. `"VNC"`.
    fn name(&self) -> &'static str;

    /// Open a session to `target`, applying the given TLS `policy`.
    ///
    /// Connectors for protocols without TLS ignore `policy`. The returned future
    /// is `Send` so the UI can drive it on a multi-threaded runtime.
    fn connect(
        &self,
        target: &Target,
        policy: &TlsPolicy,
    ) -> impl Future<Output = Result<SessionHandle, ConnectError>> + Send;
}
