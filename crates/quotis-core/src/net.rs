//! The one place Quotis talks to the network: a bounded, read-only JSON GET.

use crate::model::{ErrorKind, ProviderError};
use serde_json::Value;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY: u64 = 1 << 20;

/// GETs `url` and parses the body as JSON. Never logs headers, so tokens stay out of logs.
pub fn get(url: &str, headers: &[(&str, &str)]) -> Result<Value, ProviderError> {
    let mut req = ureq::get(url).config().http_status_as_error(false).timeout_global(Some(TIMEOUT)).max_redirects(0).build().header("User-Agent", "Quotis");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut resp = req.call().map_err(|_| ProviderError::new(ErrorKind::Offline, "offline"))?;
    match resp.status().as_u16() {
        200 => {}
        429 => {
            let retry = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()?.trim().parse().ok());
            return Err(ProviderError { retry_after_secs: retry, ..ProviderError::new(ErrorKind::RateLimited, "rate limited, retrying later") });
        }
        401 | 403 => return Err(ProviderError::new(ErrorKind::AuthExpired, "login expired")),
        code => return Err(ProviderError::new(ErrorKind::Unavailable, &format!("unavailable (HTTP {code})"))),
    }
    let bad = || ProviderError::new(ErrorKind::BadResponse, "unexpected response");
    let body = resp.body_mut().with_config().limit(MAX_BODY).read_to_string().map_err(|_| bad())?;
    serde_json::from_str(&body).map_err(|_| bad())
}

pub fn bad_response() -> ProviderError {
    ProviderError::new(ErrorKind::BadResponse, "unexpected response")
}

pub fn not_signed_in() -> ProviderError {
    ProviderError::new(ErrorKind::NotSignedIn, "not signed in")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serves one canned HTTP response on a local port and returns its URL.
    fn serve(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/usage", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let _ = s.read(&mut [0u8; 2048]);
                let _ = s.write_all(response.as_bytes());
            }
        });
        url
    }

    #[test]
    fn ok_json() {
        let url = serve("HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\n{\"a\": 1}");
        assert_eq!(get(&url, &[("Authorization", "Bearer secret")]).unwrap()["a"], 1);
    }

    #[test]
    fn rate_limit_carries_retry_after() {
        let url = serve("HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let e = get(&url, &[]).unwrap_err();
        assert_eq!((e.kind, e.retry_after_secs), (ErrorKind::RateLimited, Some(120)));
    }

    #[test]
    fn statuses_classify() {
        let kind = |resp| get(&serve(resp), &[]).unwrap_err().kind;
        assert_eq!(kind("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"), ErrorKind::AuthExpired);
        assert_eq!(kind("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"), ErrorKind::AuthExpired);
        assert_eq!(kind("HTTP/1.1 500 Oops\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"), ErrorKind::Unavailable);
        assert_eq!(kind("HTTP/1.1 302 Found\r\nLocation: http://example.invalid/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"), ErrorKind::Unavailable);
        assert_eq!(kind("HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\n<html>no!"), ErrorKind::BadResponse);
    }

    #[test]
    fn connection_refused_is_offline() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let e = get(&format!("http://127.0.0.1:{port}/"), &[]).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Offline);
    }

    #[test]
    fn errors_never_echo_the_request() {
        let url = serve("HTTP/1.1 500 Oops\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let e = get(&url, &[("Authorization", "Bearer sk-secret-token")]).unwrap_err();
        assert!(!format!("{e:?}").contains("sk-secret-token"));
    }
}
