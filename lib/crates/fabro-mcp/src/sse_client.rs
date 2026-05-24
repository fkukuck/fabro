use std::fmt;
use std::future::Future;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use fabro_http::{HeaderMap, HeaderName, HeaderValue, Url, header};
use futures::StreamExt as _;
use rmcp::RoleClient;
use rmcp::model::ServerJsonRpcMessage;
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::Transport;
use tokio::sync::{Mutex, mpsc, watch};

pub(crate) struct SseClientTransport {
    client:      fabro_http::HttpClient,
    endpoint_rx: watch::Receiver<Option<String>>,
    messages_rx: mpsc::Receiver<ServerJsonRpcMessage>,
}

impl SseClientTransport {
    pub(crate) fn new(
        url: String,
        headers: HeaderMap,
        client: fabro_http::HttpClient,
    ) -> Result<Self> {
        let (endpoint_tx, endpoint_rx) = watch::channel(None);
        let (messages_tx, messages_rx) = mpsc::channel(64);
        let sse_url = Url::parse(&url).with_context(|| format!("invalid SSE MCP URL '{url}'"))?;
        let stream_client = client.clone();

        tokio::spawn(async move {
            if let Err(err) =
                read_sse_stream(stream_client, sse_url, headers, endpoint_tx, messages_tx).await
            {
                tracing::warn!(error = %err, "SSE MCP stream ended");
            }
        });

        Ok(Self {
            client,
            endpoint_rx,
            messages_rx,
        })
    }
}

impl Transport<RoleClient> for SseClientTransport {
    type Error = SseClientError;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let client = self.client.clone();
        let mut endpoint_rx = self.endpoint_rx.clone();
        async move {
            let endpoint = wait_for_endpoint(&mut endpoint_rx).await?;
            client
                .post(endpoint)
                .header(header::CONTENT_TYPE, "application/json")
                .json(&item)
                .send()
                .await
                .map_err(SseClientError::from_error)?
                .error_for_status()
                .map_err(SseClientError::from_error)?;
            Ok(())
        }
    }

    fn receive(&mut self) -> impl Future<Output = Option<RxJsonRpcMessage<RoleClient>>> + Send {
        self.messages_rx.recv()
    }

    async fn close(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

async fn wait_for_endpoint(
    endpoint_rx: &mut watch::Receiver<Option<String>>,
) -> std::result::Result<String, SseClientError> {
    loop {
        if let Some(endpoint) = endpoint_rx.borrow().clone() {
            return Ok(endpoint);
        }
        endpoint_rx
            .changed()
            .await
            .map_err(SseClientError::from_error)?;
    }
}

async fn read_sse_stream(
    client: fabro_http::HttpClient,
    sse_url: Url,
    headers: HeaderMap,
    endpoint_tx: watch::Sender<Option<String>>,
    messages_tx: mpsc::Sender<ServerJsonRpcMessage>,
) -> Result<()> {
    let mut request = client
        .get(sse_url.clone())
        .header(header::ACCEPT, "text/event-stream");
    for (name, value) in &headers {
        request = request.header(name, value);
    }
    let response = request.send().await?.error_for_status()?;
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let state = Arc::new(Mutex::new(SseEvent::default()));

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(index) = buffer.find('\n') {
            let mut line = buffer[..index].to_string();
            buffer.drain(..=index);
            if line.ends_with('\r') {
                line.pop();
            }
            let mut event = state.lock().await;
            if let Some(complete) = event.push_line(&line) {
                drop(event);
                handle_sse_event(complete, &sse_url, &endpoint_tx, &messages_tx).await?;
            }
        }
    }

    Ok(())
}

async fn handle_sse_event(
    event: SseEvent,
    sse_url: &Url,
    endpoint_tx: &watch::Sender<Option<String>>,
    messages_tx: &mpsc::Sender<ServerJsonRpcMessage>,
) -> Result<()> {
    match event.event.as_deref() {
        Some("endpoint") => {
            let endpoint = resolve_endpoint_url(sse_url, event.data.trim())
                .with_context(|| format!("invalid SSE MCP endpoint '{}'", event.data))?;
            let _ = endpoint_tx.send(Some(endpoint.to_string()));
        }
        None | Some("") | Some("message") => {
            if event.data.trim().is_empty() {
                return Ok(());
            }
            let message: ServerJsonRpcMessage = serde_json::from_str(&event.data)
                .with_context(|| format!("invalid SSE MCP JSON-RPC message '{}'", event.data))?;
            messages_tx
                .send(message)
                .await
                .map_err(|_| anyhow!("SSE MCP receiver closed"))?;
        }
        _ => {}
    }
    Ok(())
}

fn resolve_endpoint_url(sse_url: &Url, endpoint: &str) -> Result<Url> {
    if endpoint.starts_with('/') {
        let (path, query) = endpoint
            .split_once('?')
            .map_or((endpoint, None), |(path, query)| (path, Some(query)));
        let base_path = sse_url.path().trim_end_matches('/');
        let prefix = base_path.rsplit_once('/').map_or("", |(prefix, _)| prefix);
        let mut url = sse_url.clone();
        url.set_path(&format!("{prefix}{path}"));
        url.set_query(query);
        return Ok(url);
    }

    sse_url.join(endpoint).map_err(Into::into)
}

#[derive(Default)]
struct SseEvent {
    event: Option<String>,
    data:  String,
}

impl SseEvent {
    fn push_line(&mut self, line: &str) -> Option<Self> {
        if line.is_empty() {
            if self.event.is_none() && self.data.is_empty() {
                return None;
            }
            return Some(std::mem::take(self));
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = line.split_once(':').map_or((line, ""), |(field, value)| {
            (field, value.strip_prefix(' ').unwrap_or(value))
        });
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(value);
            }
            _ => {}
        }
        None
    }
}

pub(crate) fn headers_from_pairs(
    headers: &std::collections::HashMap<String, String>,
) -> Result<HeaderMap> {
    let mut header_map = HeaderMap::new();
    for (key, value) in headers {
        let name = HeaderName::from_bytes(key.as_bytes())
            .with_context(|| format!("invalid header name '{key}'"))?;
        let val = HeaderValue::from_str(value)
            .with_context(|| format!("invalid header value for '{key}'"))?;
        header_map.insert(name, val);
    }
    Ok(header_map)
}

#[derive(Debug)]
pub(crate) struct SseClientError(String);

impl SseClientError {
    fn from_error(error: impl std::error::Error) -> Self {
        Self(error.to_string())
    }
}

impl fmt::Display for SseClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SseClientError {}
