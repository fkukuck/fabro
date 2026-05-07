use std::future::Future;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use fabro_http::{HttpClient, HttpClientBuilder};
use tokio::time;

use crate::azure::protocol::{
    ExecRequest, ExecResponse, ReadFileRequest, ReadFileResponse, WriteFileRequest,
};

pub struct SandboxdClient {
    http:     HttpClient,
    base_url: String,
}

const SANDBOXD_MAX_RETRIES: u32 = 2;
const SANDBOXD_INITIAL_RETRY_DELAY: Duration = Duration::from_millis(100);

impl SandboxdClient {
    pub fn new(base_url: String) -> Result<Self, String> {
        let http = HttpClientBuilder::new()
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self { http, base_url })
    }

    pub async fn exec(&self, request: ExecRequest) -> Result<ExecResponse, String> {
        self.retry_request("exec", || async {
            self.http
                .post(format!("{}/exec", self.base_url))
                .json(&request)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await
        })
        .await
    }

    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let response: ReadFileResponse = self
            .retry_request("read_file", || async {
                self.http
                    .post(format!("{}/read-file", self.base_url))
                    .json(&ReadFileRequest {
                        path: path.to_string(),
                    })
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await
            })
            .await?;
        STANDARD
            .decode(response.content_base64)
            .map_err(|err| err.to_string())
    }

    pub async fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.retry_request("write_file", || async {
            self.http
                .post(format!("{}/write-file", self.base_url))
                .json(&WriteFileRequest {
                    path:           path.to_string(),
                    content_base64: STANDARD.encode(bytes),
                })
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    pub async fn health(&self) -> Result<(), String> {
        self.retry_request("health", || async {
            self.http
                .get(format!("{}/health", self.base_url))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    async fn retry_request<T, F, Fut>(&self, operation: &str, mut request: F) -> Result<T, String>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, reqwest::Error>>,
    {
        let mut attempt = 0;

        loop {
            match request().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    if attempt >= SANDBOXD_MAX_RETRIES || !is_retryable_http_error(&err) {
                        return Err(err.to_string());
                    }

                    let delay = retry_delay(attempt);
                    tracing::warn!(
                        operation,
                        attempt = attempt + 1,
                        delay_ms = delay.as_millis(),
                        error = %err,
                        "sandboxd request failed, retrying"
                    );
                    time::sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }
}

fn is_retryable_http_error(err: &reqwest::Error) -> bool {
    err.is_timeout()
        || err.is_connect()
        || err.is_body()
        || err.is_decode()
        || err
            .status()
            .is_some_and(|status| status.is_server_error() || status.as_u16() == 429)
}

fn retry_delay(attempt: u32) -> Duration {
    SANDBOXD_INITIAL_RETRY_DELAY.saturating_mul(attempt + 1)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    async fn spawn_sequence_server(responses: Vec<String>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_task = Arc::clone(&hits);

        tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                hits_for_task.fetch_add(1, Ordering::SeqCst);

                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request).await.unwrap();
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });

        (format!("http://{addr}"), hits)
    }

    fn http_response(status: &str, content_type: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn read_file_retries_decode_error() {
        let valid_body =
            serde_json::json!({ "content_base64": STANDARD.encode("hello") }).to_string();
        let responses = vec![
            http_response("200 OK", "application/json", "not json"),
            http_response("200 OK", "application/json", &valid_body),
        ];
        let (base_url, hits) = spawn_sequence_server(responses).await;
        let client = SandboxdClient::new(base_url).unwrap();

        let content = client.read_file("/workspace/test.txt").await.unwrap();

        assert_eq!(content, b"hello");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn write_file_retries_server_error() {
        let responses = vec![
            http_response(
                "500 Internal Server Error",
                "text/plain",
                "temporary failure",
            ),
            http_response("200 OK", "text/plain", ""),
        ];
        let (base_url, hits) = spawn_sequence_server(responses).await;
        let client = SandboxdClient::new(base_url).unwrap();

        client
            .write_file("/workspace/test.txt", b"hello")
            .await
            .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn exec_retries_decode_error() {
        let valid_body = serde_json::json!({
            "stdout": "ok",
            "stderr": "",
            "exit_code": 0,
            "timed_out": false,
            "duration_ms": 12
        })
        .to_string();
        let responses = vec![
            http_response("200 OK", "application/json", "not json"),
            http_response("200 OK", "application/json", &valid_body),
        ];
        let (base_url, hits) = spawn_sequence_server(responses).await;
        let client = SandboxdClient::new(base_url).unwrap();

        let response = client
            .exec(ExecRequest {
                command:     "pwd".to_string(),
                working_dir: None,
                env:         std::collections::HashMap::new(),
                timeout_ms:  1000,
            })
            .await
            .unwrap();

        assert_eq!(response.stdout, "ok");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }
}
