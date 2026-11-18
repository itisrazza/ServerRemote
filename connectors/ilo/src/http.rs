//! A minimal HTTPS client for the iLO's web API.
//!
//! The iLO's JSON API needs only a handful of requests, each answered with a
//! small JSON body, so this speaks just enough HTTP/1.1 over
//! [`server_remote_core::tls::connect`]: one request per connection
//! (`Connection: close`), bodies framed by `Content-Length`, chunked encoding
//! or the end of the stream. Going through the core TLS transport keeps the
//! trust-on-first-use policy in charge of the iLO's (usually self-signed)
//! certificate.

use std::io;

use server_remote_core::{ConnectError, Target, TlsPolicy, tls};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

/// The largest response this client accepts. The iLO's JSON replies are a
/// few hundred bytes; anything this large is not one of them.
const MAX_RESPONSE: usize = 1 << 20;

/// An HTTP response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    /// Header names are lower-cased; repeated headers appear once each.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// The value of the cookie `name`, if a `Set-Cookie` header sets it.
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .filter(|(header, _)| header == "set-cookie")
            .filter_map(|(_, value)| value.split(';').next())
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| key.trim() == name)
            .map(|(_, value)| value.trim())
    }
}

/// Sends one request over a fresh TLS connection and reads the response.
pub async fn request(
    target: &Target,
    policy: &TlsPolicy,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Result<Response, ConnectError> {
    let mut stream = tls::connect(target, policy).await?;

    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n",
        host_header(target)
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        head.push_str("Content-Type: application/json; charset=utf-8\r\n");
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");

    stream.write_all(head.as_bytes()).await?;
    if let Some(body) = body {
        stream.write_all(body).await?;
    }
    stream.flush().await?;

    let raw = read_to_end(&mut stream).await?;
    parse_response(&raw).map_err(|message| ConnectError::Protocol(format!("HTTP: {message}")))
}

/// `host[:port]`, with IPv6 addresses bracketed and the default port left off.
fn host_header(target: &Target) -> String {
    let host = if target.host.contains(':') {
        format!("[{}]", target.host)
    } else {
        target.host.clone()
    };
    if target.port == 443 {
        host
    } else {
        format!("{host}:{}", target.port)
    }
}

/// Reads until the peer closes. A peer that closes without a TLS
/// `close_notify` (common for embedded servers) still ends the response.
async fn read_to_end(stream: &mut (impl AsyncRead + Unpin)) -> Result<Vec<u8>, ConnectError> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > MAX_RESPONSE {
                    return Err(ConnectError::Protocol("HTTP response too large".into()));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof && !raw.is_empty() => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(raw)
}

fn parse_response(raw: &[u8]) -> Result<Response, String> {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("no end of headers")?;
    let head = std::str::from_utf8(&raw[..split]).map_err(|_| "headers aren't UTF-8")?;
    let rest = &raw[split + 4..];

    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or("empty response")?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| format!("bad status line {status_line:?}"))?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();

    let header = |name: &str| {
        headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    };
    let body = if header("transfer-encoding").is_some_and(|value| value.contains("chunked")) {
        dechunk(rest)?
    } else if let Some(length) = header("content-length") {
        let length: usize = length.parse().map_err(|_| "bad Content-Length")?;
        rest.get(..length)
            .ok_or("body shorter than Content-Length")?
            .to_vec()
    } else {
        rest.to_vec()
    };

    Ok(Response {
        status,
        headers,
        body,
    })
}

fn dechunk(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let line_end = data
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or("truncated chunk size")?;
        let size_text = std::str::from_utf8(&data[..line_end]).map_err(|_| "bad chunk size")?;
        let size_text = size_text.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16).map_err(|_| "bad chunk size")?;
        data = &data[line_end + 2..];
        if size == 0 {
            return Ok(body);
        }
        body.extend_from_slice(data.get(..size).ok_or("truncated chunk")?);
        data = data.get(size + 2..).ok_or("truncated chunk")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_content_length_response_and_its_cookie() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/x-javascript\r\n\
            Set-Cookie: sessionKey=abc123; path=/; secure\r\nContent-Length: 2\r\n\r\n{}trailing";
        let response = parse_response(raw).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"{}");
        assert_eq!(response.cookie("sessionKey"), Some("abc123"));
        assert_eq!(response.cookie("MPID"), None);
    }

    #[test]
    fn parses_a_chunked_response() {
        let raw = b"HTTP/1.1 403 Forbidden\r\nTransfer-Encoding: chunked\r\n\r\n\
            4\r\n{\"a\"\r\n3;ext\r\n:1}\r\n0\r\n\r\n";
        let response = parse_response(raw).unwrap();
        assert_eq!(response.status, 403);
        assert_eq!(response.body, b"{\"a\":1}");
    }

    #[test]
    fn a_response_without_length_runs_to_the_end() {
        let response = parse_response(b"HTTP/1.0 500 Error\r\n\r\n").unwrap();
        assert_eq!(response.status, 500);
        assert!(response.body.is_empty());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_response(b"not http").is_err());
        assert!(parse_response(b"HTTP/1.1 abc\r\n\r\n").is_err());
    }

    #[test]
    fn host_header_brackets_ipv6_and_drops_the_default_port() {
        assert_eq!(host_header(&Target::new("ilo", 443)), "ilo");
        assert_eq!(host_header(&Target::new("ilo", 8443)), "ilo:8443");
        assert_eq!(host_header(&Target::new("fe80::1", 443)), "[fe80::1]");
    }
}
