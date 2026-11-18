# TLS trust

Some targets (iLO especially) ship self-signed or expired certificates, so
connecting insecurely must be possible. By default it is trust-on-first-use:
the user approves a *specific* certificate. Accepting any certificate is a
separate policy the user has to choose in Settings.

```rust
pub enum TlsPolicy {
    Secure,
    AllowInsecure { accepted: Option<CertFingerprint> },
    AcceptAny,
}
```

Under `Secure`, or `AllowInsecure` with no matching `accepted` fingerprint, a
connector that hits an invalid certificate returns
`ConnectError::UntrustedCertificate(fingerprint)`. The UI decides what to do
with it (see below). `AcceptAny` accepts every certificate.

## Transport

`server_remote_core::tls::connect(target, policy)` opens TCP to the target and
negotiates TLS with `rustls` (the `aws-lc-rs` provider, TLS 1.2 and 1.3). Its
verifier checks the certificate against the Mozilla roots (`webpki-roots`) as
usual, except that a certificate the policy accepts passes without those
checks. Handshake signatures are always verified, so the peer still has to
hold the certificate's private key, even under `AcceptAny`.

For an accepted certificate only, TLS 1.2 handshake signatures may also use a
legacy RSA key (1024 bits and up, which older iLO firmware generates) and
SHA-1, which the verifier then advertises. Without this, such a server fails
with a misleading `BadSignature` even after the user approves it. Fixtures for
the test live in `lib/core/testdata/legacy-rsa/`, with the script that made
them.

Connectors use this rather than building their own TLS; the iLO connector's
HTTPS client runs on it.

## In the UI

**Settings › Security › Certificate checking** picks the policy each new
connection starts with:

| Setting | Policy | An unrecognised certificate… |
|---|---|---|
| Strict | `Secure` | is refused, with the reason on the disconnected screen. |
| Ask (default) | `AllowInsecure`, with the trusted fingerprint for that host and port, if any | prompts, unless it's blocked. |
| Off | `AcceptAny` | is accepted without asking, and not recorded. |

The prompt shows the fingerprint (colon-separated hex, as browsers show it)
and offers:

- **Block**: adds it to the blocked list and stays disconnected. A blocked
  certificate is refused without asking, under Strict or Ask.
- **Cancel**: stays disconnected.
- **Connect once**: reconnects trusting that fingerprint, for this tab only.
- **Always trust**: also saves it to the trusted list, replacing any other
  certificate for that host and port.

Both lists live in the `[security]` section of `settings.toml`, keyed by host
and port, and can be pruned on the Security page. The application, not the
connection tab, applies them: it intercepts `UntrustedCertificate`,
`TrustCertificate` and `BlockCertificate` on their way to the tab.
