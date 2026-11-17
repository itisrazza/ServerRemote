# TLS trust

Some targets (iLO especially) ship self-signed or expired certificates, so
connecting insecurely must be possible — but never as a blanket "skip
verification" switch. Instead it is trust-on-first-use:

```rust
pub enum TlsPolicy {
    Secure,
    AllowInsecure { accepted: Option<CertFingerprint> },
}
```

When `accepted` is `None`, a connector that hits an invalid certificate returns
`ConnectError::UntrustedCertificate(fingerprint)`. The UI surfaces that
fingerprint, the user approves it, and the approved fingerprint is what gets
trusted on the next attempt — a specific certificate, not "anything goes".
