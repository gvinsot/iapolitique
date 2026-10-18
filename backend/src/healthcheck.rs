//! `iapolitique healthcheck`: minimal HTTP/1.1 GET used by Docker healthchecks, so that the
//! runtime image needs neither curl nor wget. Exit code 0 when the endpoint answers 2xx.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum HealthcheckError {
    #[error("invalid URL: {0}")]
    InvalidUrl(&'static str),
    #[error("cannot resolve or connect to {0}")]
    Unreachable(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("malformed HTTP response")]
    MalformedResponse,
    #[error("unhealthy: HTTP status {0}")]
    Status(u16),
}

/// Target of a health check, parsed from an `http://host[:port]/path` URL.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub path: String,
}

/// Parses a plain `http://` URL (TLS is never needed: checks target the local container).
pub fn parse_url(url: &str) -> Result<Target, HealthcheckError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or(HealthcheckError::InvalidUrl(
            "only http:// URLs are supported",
        ))?;
    let (authority, path) = match rest.find('/') {
        Some(pos) => (&rest[..pos], &rest[pos..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') {
        return Err(HealthcheckError::InvalidUrl("missing or invalid host"));
    }
    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        // IPv6 literal: [::1]:8080
        let (host, after) = stripped
            .split_once(']')
            .ok_or(HealthcheckError::InvalidUrl("unterminated IPv6 address"))?;
        let port = match after.strip_prefix(':') {
            Some(p) => p,
            None if after.is_empty() => "80",
            None => return Err(HealthcheckError::InvalidUrl("invalid port")),
        };
        (host, port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, port),
            None => (authority, "80"),
        }
    };
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or(HealthcheckError::InvalidUrl("invalid port"))?;
    if host.is_empty() || path.contains(['\r', '\n', ' ']) {
        return Err(HealthcheckError::InvalidUrl("missing host or invalid path"));
    }
    Ok(Target {
        host: host.to_owned(),
        port,
        path: path.to_owned(),
    })
}

/// Performs the GET and returns the (2xx) status code.
pub fn check(url: &str, timeout: Duration) -> Result<u16, HealthcheckError> {
    let target = parse_url(url)?;
    let unreachable = || HealthcheckError::Unreachable(format!("{}:{}", target.host, target.port));
    let addrs: Vec<SocketAddr> = (target.host.as_str(), target.port)
        .to_socket_addrs()
        .map_err(|_| unreachable())?
        .collect();
    let mut stream = addrs
        .iter()
        .find_map(|addr| TcpStream::connect_timeout(addr, timeout).ok())
        .ok_or_else(unreachable)?;
    let io = |err: std::io::Error| HealthcheckError::Io(err.to_string());
    stream.set_read_timeout(Some(timeout)).map_err(io)?;
    stream.set_write_timeout(Some(timeout)).map_err(io)?;

    let host_header = if target.host.contains(':') {
        format!("[{}]:{}", target.host, target.port)
    } else {
        format!("{}:{}", target.host, target.port)
    };
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: iapolitique-healthcheck\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        target.path
    );
    stream.write_all(request.as_bytes()).map_err(io)?;

    // Only the status line matters.
    let mut buf = Vec::with_capacity(256);
    let mut chunk = [0u8; 256];
    while !buf.windows(2).any(|w| w == b"\r\n") && buf.len() < 4096 {
        let n = stream.read(&mut chunk).map_err(io)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let status = parse_status_line(&buf)?;
    if (200..300).contains(&status) {
        Ok(status)
    } else {
        Err(HealthcheckError::Status(status))
    }
}

fn parse_status_line(buf: &[u8]) -> Result<u16, HealthcheckError> {
    let line_end = buf
        .windows(2)
        .position(|w| w == b"\r\n")
        .ok_or(HealthcheckError::MalformedResponse)?;
    let line =
        std::str::from_utf8(&buf[..line_end]).map_err(|_| HealthcheckError::MalformedResponse)?;
    let mut parts = line.split(' ');
    match (parts.next(), parts.next()) {
        (Some(version), Some(code)) if version.starts_with("HTTP/") && code.len() == 3 => code
            .parse()
            .map_err(|_| HealthcheckError::MalformedResponse),
        _ => Err(HealthcheckError::MalformedResponse),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn serve_once(response: &'static str) -> (u16, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut chunk = [0u8; 512];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..n]);
            }
            socket.write_all(response.as_bytes()).unwrap();
            String::from_utf8(request).unwrap()
        });
        (port, handle)
    }

    #[test]
    fn parses_urls() {
        assert_eq!(
            parse_url("http://127.0.0.1:8082/health").unwrap(),
            Target {
                host: "127.0.0.1".into(),
                port: 8082,
                path: "/health".into()
            }
        );
        assert_eq!(
            parse_url("http://localhost").unwrap(),
            Target {
                host: "localhost".into(),
                port: 80,
                path: "/".into()
            }
        );
        assert_eq!(
            parse_url("http://[::1]:8080/api/health").unwrap().host,
            "::1"
        );
        assert!(parse_url("https://localhost/").is_err());
        assert!(parse_url("http://:80/").is_err());
        assert!(parse_url("http://host:0/").is_err());
        assert!(parse_url("http://host:99999/").is_err());
        assert!(parse_url("http://user@host/").is_err());
        assert!(parse_url("http://host/a b").is_err());
    }

    #[test]
    fn healthy_endpoint() {
        let (port, server) = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        let status = check(
            &format!("http://127.0.0.1:{port}/api/health"),
            Duration::from_secs(2),
        );
        assert_eq!(status, Ok(200));
        let request = server.join().unwrap();
        assert!(request.starts_with("GET /api/health HTTP/1.1\r\n"));
        assert!(request.contains(&format!("Host: 127.0.0.1:{port}\r\n")));
    }

    #[test]
    fn unhealthy_endpoint() {
        let (port, server) = serve_once("HTTP/1.1 503 Service Unavailable\r\n\r\n");
        let status = check(&format!("http://127.0.0.1:{port}/"), Duration::from_secs(2));
        assert_eq!(status, Err(HealthcheckError::Status(503)));
        server.join().unwrap();
    }

    #[test]
    fn garbage_response() {
        let (port, server) = serve_once("hello\r\n");
        let status = check(&format!("http://127.0.0.1:{port}/"), Duration::from_secs(2));
        assert_eq!(status, Err(HealthcheckError::MalformedResponse));
        server.join().unwrap();
    }

    #[test]
    fn closed_port() {
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let status = check(
            &format!("http://127.0.0.1:{port}/"),
            Duration::from_millis(500),
        );
        assert!(matches!(status, Err(HealthcheckError::Unreachable(_))));
    }
}
