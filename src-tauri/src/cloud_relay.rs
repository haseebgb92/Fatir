use base64::{engine::general_purpose::STANDARD as B64, Engine};
use futures_util::{SinkExt, StreamExt};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        http::{header::AUTHORIZATION, HeaderValue},
        Message,
    },
};
use url::Url;

const LOCAL_BASE: &str = "http://127.0.0.1:32145";
const MAX_TUNNEL_BODY: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    pub enabled: bool,
    pub url: String,
    pub device_id: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelayStatus {
    pub configured: bool,
    pub enabled: bool,
    pub relay_url: Option<String>,
    pub device_id: Option<String>,
    pub cloud_base_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TunnelMessage {
    Request(TunnelRequest),
    Response(TunnelResponse),
    Ping { nonce: String },
    Pong { nonce: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TunnelRequest {
    id: String,
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TunnelResponse {
    id: String,
    status: u16,
    headers: HashMap<String, String>,
    body_b64: String,
    #[serde(default)]
    error: Option<String>,
}

pub fn config_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
        .join("companion")
        .join("relay.json")
}

pub fn load_config() -> Option<RelayConfig> {
    let raw = fs::read_to_string(config_path()).ok()?;
    let config: RelayConfig = serde_json::from_str(&raw).ok()?;
    if config.url.trim().is_empty()
        || config.device_id.trim().len() < 16
        || config.token.trim().len() < 32
    {
        return None;
    }
    Some(config)
}

pub fn status() -> RelayStatus {
    let config = load_config();
    RelayStatus {
        configured: config.is_some(),
        enabled: config.as_ref().map(|c| c.enabled).unwrap_or(false),
        relay_url: config.as_ref().map(|c| c.url.clone()),
        device_id: config.as_ref().map(|c| c.device_id.clone()),
        cloud_base_url: config
            .as_ref()
            .and_then(|c| cloud_base_url(c).ok()),
    }
}

pub async fn run_forever(local_token: String) {
    let mut backoff = 2_u64;
    loop {
        let Some(config) = load_config() else {
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        };
        if !config.enabled {
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        }

        match run_once(config, local_token.clone()).await {
            Ok(()) => backoff = 2,
            Err(err) => {
                eprintln!("Fatir cloud relay disconnected: {err}");
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
            }
        }
    }
}

async fn run_once(config: RelayConfig, local_token: String) -> anyhow::Result<()> {
    let ws_url = desktop_ws_url(&config)?;
    let mut request = ws_url.as_str().into_client_request()?;
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", config.token))?,
    );

    let (socket, _) = connect_async(request).await?;
    eprintln!(
        "Fatir cloud relay connected: {} ({})",
        config.url, config.device_id
    );

    let (mut socket_tx, mut socket_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if socket_tx.send(message).await.is_err() {
                break;
            }
        }
    });

    while let Some(message) = socket_rx.next().await {
        match message? {
            Message::Text(text) => {
                if let Ok(parsed) = serde_json::from_str::<TunnelMessage>(&text) {
                    match parsed {
                        TunnelMessage::Request(request) => {
                            let tx = tx.clone();
                            let local_token = local_token.clone();
                            tokio::spawn(async move {
                                let response = forward_local(request, &local_token).await;
                                if let Ok(text) = serde_json::to_string(&TunnelMessage::Response(response)) {
                                    let _ = tx.send(Message::Text(text));
                                }
                            });
                        }
                        TunnelMessage::Ping { nonce } => {
                            if let Ok(text) = serde_json::to_string(&TunnelMessage::Pong { nonce }) {
                                let _ = tx.send(Message::Text(text));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Message::Ping(payload) => {
                let _ = tx.send(Message::Pong(payload));
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    writer.abort();
    Ok(())
}

async fn forward_local(request: TunnelRequest, local_token: &str) -> TunnelResponse {
    let id = request.id.clone();
    match forward_local_inner(request, local_token).await {
        Ok(mut response) => {
            response.id = id;
            response
        }
        Err(err) => TunnelResponse {
            id,
            status: 502,
            headers: HashMap::new(),
            body_b64: String::new(),
            error: Some(err.to_string()),
        },
    }
}

async fn forward_local_inner(
    request: TunnelRequest,
    local_token: &str,
) -> anyhow::Result<TunnelResponse> {
    if !request.path.starts_with("/api/") {
        anyhow::bail!("Relay refused non-API path");
    }

    let method = Method::from_bytes(request.method.as_bytes())?;
    let url = format!("{LOCAL_BASE}{}", request.path);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(12 * 60))
        .build()?;

    let mut builder = client
        .request(method, url)
        .header("Authorization", format!("Bearer {local_token}"));

    for (name, value) in request.headers {
        if matches!(
            name.as_str(),
            "content-type" | "accept" | "x-fatir-filename"
        ) {
            builder = builder.header(name, value);
        }
    }

    let body = B64.decode(request.body_b64.as_bytes())?;
    if body.len() > MAX_TUNNEL_BODY {
        anyhow::bail!("Cloud relay request exceeds 32 MB");
    }

    let response = builder.body(body).send().await?;
    let status = response.status().as_u16();
    let mut headers = HashMap::new();
    for name in ["content-type", "content-disposition"] {
        if let Some(value) = response.headers().get(name).and_then(|v| v.to_str().ok()) {
            headers.insert(name.to_string(), value.to_string());
        }
    }

    let content_length = response.content_length();
    if content_length.unwrap_or(0) > MAX_TUNNEL_BODY as u64 {
        anyhow::bail!("Cloud relay response exceeds 32 MB");
    }

    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if bytes.len().saturating_add(chunk.len()) > MAX_TUNNEL_BODY {
            anyhow::bail!("Cloud relay response exceeds 32 MB");
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok(TunnelResponse {
        id: String::new(),
        status,
        headers,
        body_b64: B64.encode(bytes),
        error: None,
    })
}

fn desktop_ws_url(config: &RelayConfig) -> anyhow::Result<Url> {
    let mut url = Url::parse(config.url.trim_end_matches('/'))?;
    match url.scheme() {
        "https" => {
            url.set_scheme("wss")
                .map_err(|_| anyhow::anyhow!("Invalid HTTPS relay URL"))?;
        }
        "http" => {
            url.set_scheme("ws")
                .map_err(|_| anyhow::anyhow!("Invalid HTTP relay URL"))?;
        }
        "wss" | "ws" => {}
        _ => anyhow::bail!("Relay URL must use https/http/wss/ws"),
    }
    url.set_path(&format!("/ws/desktop/{}", config.device_id));
    url.set_query(None);
    Ok(url)
}

fn cloud_base_url(config: &RelayConfig) -> anyhow::Result<String> {
    let mut url = Url::parse(config.url.trim_end_matches('/'))?;
    match url.scheme() {
        "https" | "http" => {}
        "wss" => {
            url.set_scheme("https")
                .map_err(|_| anyhow::anyhow!("Invalid WSS relay URL"))?;
        }
        "ws" => {
            url.set_scheme("http")
                .map_err(|_| anyhow::anyhow!("Invalid WS relay URL"))?;
        }
        _ => anyhow::bail!("Invalid relay URL scheme"),
    }
    url.set_path(&format!("/d/{}", config.device_id));
    url.set_query(None);
    Ok(url.to_string().trim_end_matches('/').to_string())
}
