//! TLS trust policy.
//!
//! Some targets — iLO especially — ship self-signed or expired certificates, so
//! connecting insecurely has to be possible. It must never be a blanket "skip
//! verification" toggle though: the user accepts a *specific* certificate
//! (trust-on-first-use), and only that certificate is then trusted.

/// SHA-256 fingerprint of a peer certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CertFingerprint(pub [u8; 32]);

/// How strictly a peer's TLS certificate is validated.
#[derive(Debug, Clone, Default)]
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
