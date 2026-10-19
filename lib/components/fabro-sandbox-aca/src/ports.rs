//! Private routes to ports inside a sandbox, for sandbox-hosted MCP
//! servers.
//!
//! ACA can expose a port only through public ingress, and nothing a run
//! starts may be public. A route is therefore a listener on this machine's
//! loopback that relays each HTTP request through a command in the
//! sandbox: the command sends the request to `127.0.0.1:<port>` there and
//! prints the raw response. Requests are relayed one at a time per
//! connection, which is what MCP's streamable HTTP transport needs; a
//! response is buffered whole, so a server-sent event stream that never
//! ends does not fit through a route.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sandbox_driver::{Error, Exec, ExecSpec, PreviewUrl, PreviewUrls, ProviderError, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::client::kind;

/// How long one relayed request may take in the sandbox.
const RELAY_TIMEOUT: Duration = Duration::from_mins(5);
/// The largest request body relayed.
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Sends the request in stdin to `127.0.0.1:$1` and prints the raw
/// response, with Python's standard library.
const RELAY_SCRIPT: &str = r#"port=$1
if command -v python3 >/dev/null 2>&1; then
  exec python3 -c '
import socket, sys
data = sys.stdin.buffer.read()
s = socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=300)
s.sendall(data)
out = sys.stdout.buffer
while True:
    chunk = s.recv(65536)
    if not chunk:
        break
    out.write(chunk)
out.flush()
' "$port"
fi
echo "sandbox-driver: python3 is needed to reach a port in the sandbox" >&2
exit 127
"#;

/// The routes one sandbox handle has opened, by port.
pub(crate) struct AcaPorts {
    exec:   Arc<dyn Exec>,
    routes: Mutex<BTreeMap<u16, Route>>,
}

struct Route {
    url:  String,
    task: JoinHandle<()>,
}

impl AcaPorts {
    pub(crate) fn new(exec: Arc<dyn Exec>) -> Self {
        Self {
            exec,
            routes: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Drop for AcaPorts {
    fn drop(&mut self) {
        for (_, route) in
            std::mem::take(&mut *self.routes.lock().unwrap_or_else(PoisonError::into_inner))
        {
            route.task.abort();
        }
    }
}

#[async_trait]
impl PreviewUrls for AcaPorts {
    async fn preview_url(&self, port: u16) -> Result<PreviewUrl> {
        if let Some(route) = self
            .routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&port)
        {
            return Ok(PreviewUrl::new(route.url.clone()));
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|error| Error::io("opening a local route to a sandbox port", error))?;
        let address = listener
            .local_addr()
            .map_err(|error| Error::io("reading the local route's address", error))?;
        let url = format!("http://{address}");
        let exec = Arc::clone(&self.exec);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let exec = Arc::clone(&exec);
                tokio::spawn(async move {
                    if let Err(error) = serve(stream, &*exec, port).await {
                        tracing::debug!(%error, port, "ACA port relay connection ended");
                    }
                });
            }
        });
        let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
        // Another caller may have opened the same port meanwhile; keep one.
        if let Some(existing) = routes.get(&port) {
            task.abort();
            return Ok(PreviewUrl::new(existing.url.clone()));
        }
        routes.insert(port, Route {
            url: url.clone(),
            task,
        });
        Ok(PreviewUrl::new(url))
    }

    async fn release_preview_url(&self, port: u16) -> Result<()> {
        if let Some(route) = self
            .routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&port)
        {
            route.task.abort();
        }
        Ok(())
    }
}

/// Relays every request on one connection, in order.
async fn serve(stream: TcpStream, exec: &dyn Exec, port: u16) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    loop {
        let Some(request) = read_request(&mut reader).await? else {
            return Ok(());
        };
        let response = match relay(exec, port, &request).await {
            Ok(response) => response,
            Err(error) => error_response(&error.to_string()),
        };
        write.write_all(&response).await?;
        write.flush().await?;
    }
}

/// One HTTP/1.1 request, re-encoded for the sandbox: the request line, the
/// headers with `Connection: close` (the relay is one exchange per
/// command), and the body.
async fn read_request<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut head = Vec::new();
    let mut content_length = 0_usize;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
        if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        }
        if lower.starts_with("connection:") || lower.starts_with("keep-alive:") {
            continue;
        }
        head.push(line);
    }
    let body = if chunked {
        read_chunked(reader).await?
    } else {
        if content_length > MAX_BODY_BYTES {
            return Err(std::io::Error::other("request body too large to relay"));
        }
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body).await?;
        body
    };
    let mut request = Vec::new();
    for line in head {
        if chunked && line.to_ascii_lowercase().starts_with("transfer-encoding:") {
            continue;
        }
        request.extend_from_slice(line.as_bytes());
    }
    if chunked {
        request.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    }
    request.extend_from_slice(b"Connection: close\r\n\r\n");
    request.extend_from_slice(&body);
    Ok(Some(request))
}

async fn read_chunked<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let mut size = String::new();
        reader.read_line(&mut size).await?;
        let size = usize::from_str_radix(size.trim().split(';').next().unwrap_or("0"), 16)
            .map_err(std::io::Error::other)?;
        if size == 0 {
            let mut trailer = String::new();
            while reader.read_line(&mut trailer).await? > 2 {
                trailer.clear();
            }
            return Ok(body);
        }
        if body.len() + size > MAX_BODY_BYTES {
            return Err(std::io::Error::other("request body too large to relay"));
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..]).await?;
        let mut crlf = [0; 2];
        reader.read_exact(&mut crlf).await?;
    }
}

/// Sends `request` to the port inside the sandbox and returns the raw
/// response, framed for the local client: the sandbox's server closes the
/// connection, so its response is rewritten with a length.
async fn relay(exec: &dyn Exec, port: u16, request: &[u8]) -> Result<Vec<u8>> {
    let spec = ExecSpec::new("sh")
        .args([
            "-c",
            RELAY_SCRIPT,
            "sandbox-driver-relay",
            &port.to_string(),
        ])
        .stdin(request.to_vec())
        .timeout(RELAY_TIMEOUT)
        .working_dir("/");
    let result = exec.run(&spec).await?;
    if !result.success() || result.stdout.is_empty() {
        return Err(Error::Provider(ProviderError::new(
            kind(),
            format!(
                "port {port} in the sandbox did not answer: {}",
                result.stderr_lossy().trim()
            ),
        )));
    }
    Ok(reframe(&result.stdout))
}

/// A complete response with its body length stated, so the local client
/// can keep the connection: a chunked body is decoded first.
fn reframe(raw: &[u8]) -> Vec<u8> {
    let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
        return raw.to_vec();
    };
    let head = String::from_utf8_lossy(&raw[..split]);
    let mut body = raw[split + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or("HTTP/1.1 502 Bad Gateway");
    let mut headers = Vec::new();
    let mut chunked = false;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("transfer-encoding:") {
            chunked = lower.contains("chunked");
            continue;
        }
        if lower.starts_with("content-length:") || lower.starts_with("connection:") {
            continue;
        }
        headers.push(line.to_owned());
    }
    if chunked {
        body = dechunk(&body);
    }
    let mut out = format!("{status}\r\n").into_bytes();
    for header in headers {
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    out.extend_from_slice(&body);
    out
}

fn dechunk(mut raw: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    while let Some(end) = raw.windows(2).position(|window| window == b"\r\n") {
        let size = std::str::from_utf8(&raw[..end])
            .ok()
            .and_then(|text| usize::from_str_radix(text.split(';').next()?.trim(), 16).ok())
            .unwrap_or(0);
        raw = &raw[end + 2..];
        if size == 0 || raw.len() < size {
            break;
        }
        body.extend_from_slice(&raw[..size]);
        raw = raw.get(size + 2..).unwrap_or_default();
    }
    body
}

fn error_response(message: &str) -> Vec<u8> {
    let body = BASE64.encode(message.as_bytes());
    let body = format!("{{\"error\":\"sandbox port relay failed\",\"detail_base64\":\"{body}\"}}");
    format!(
        "HTTP/1.1 502 Bad Gateway\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_request_is_reencoded_to_close_after_one_exchange() {
        let raw = b"POST /mcp HTTP/1.1\r\nHost: x\r\nConnection: keep-alive\r\nContent-Length: 4\r\n\r\nbody";
        let mut reader = BufReader::new(&raw[..]);
        let request = read_request(&mut reader).await.unwrap().unwrap();
        let text = String::from_utf8(request).unwrap();
        assert!(text.starts_with("POST /mcp HTTP/1.1\r\n"));
        assert!(text.contains("Connection: close\r\n\r\nbody"));
        assert!(!text.contains("keep-alive"));
    }

    #[tokio::test]
    async fn a_chunked_request_body_is_sent_with_a_length() {
        let raw = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        let mut reader = BufReader::new(&raw[..]);
        let text = String::from_utf8(read_request(&mut reader).await.unwrap().unwrap()).unwrap();
        assert!(text.contains("Content-Length: 5\r\n"), "{text}");
        assert!(text.ends_with("abcde"));
        assert!(!text.to_ascii_lowercase().contains("transfer-encoding"));
    }

    #[test]
    fn a_chunked_response_is_reframed_with_its_length() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n2\r\n{}\r\n0\r\n\r\n";
        let text = String::from_utf8(reframe(raw)).unwrap();
        assert_eq!(
            text,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}"
        );
    }
}
