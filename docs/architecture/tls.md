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

## Transport

`server_remote_core::tls::connect(target, policy)` opens TCP to the target and
negotiates TLS with `rustls` (the `aws-lc-rs` provider, TLS 1.2 and 1.3). Its
verifier checks the certificate against the Mozilla roots (`webpki-roots`) as
usual, except that a certificate whose SHA-256 fingerprint the policy accepts
passes without those checks. Handshake signatures are always verified, so the
peer still has to hold the accepted certificate's private key.

For an accepted certificate only, TLS 1.2 handshake signatures may also use a
legacy RSA key (1024 bits and up, which older iLO firmware generates) and
SHA-1, which the verifier then advertises. Without this, such a server fails
with a misleading `BadSignature` even after the user approves it. Fixtures for
the test live in `lib/core/testdata/legacy-rsa/`, with the script that made
them.

Any certificate that fails, under any policy, is reported as
`UntrustedCertificate` with its fingerprint, so the UI can ask about it.
Connectors use this rather than building their own TLS; the iLO connector's
HTTPS client runs on it.

## In the UI

A connection tab that gets `UntrustedCertificate` shows the fingerprint
(colon-separated hex, as browsers show it) in a dialog. **Trust and connect**
sets the tab's policy to `AllowInsecure { accepted: Some(fingerprint) }` and
connects again; **Cancel** leaves the tab disconnected. The approval lasts for
that tab only; saved sessions don't remember it yet.
