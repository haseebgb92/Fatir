use axum::{
    body::{to_bytes, Body},
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Request, State,
    },
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, RwLock};
use uuid::Uuid;

const DEFAULT_PORT: u16 = 8787;
const DEFAULT_MAX_BODY: usize = 32 * 1024 * 1024;
const RPC_TIMEOUT: Duration = Duration::from_secs(12 * 60);

#[derive(Clone, Default)]
struct AppState {
    devices: Arc<RwLock<HashMap<String, DeviceSession>>>,
    pending: Arc<RwLock<HashMap<String, oneshot::Sender<TunnelResponse>>>>,
    max_body: usize,
}

#[derive(Clone)]
struct DeviceSession {
    token: Arc<String>,
    tx: mpsc::UnboundedSender<Message>,
    session_id: String,
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

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let max_body = std::env::var("FATIR_RELAY_MAX_BODY_MB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|mb| mb.clamp(1, 256) * 1024 * 1024)
        .unwrap_or(DEFAULT_MAX_BODY);

    let state = AppState {
        max_body,
        ..Default::default()
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ws/desktop/:device_id", get(desktop_ws))
        .route("/d/:device_id/*path", any(proxy))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind Fatir relay");
    println!("Fatir relay listening on {addr}");
    axum::serve(listener, app).await.expect("serve Fatir relay");
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let devices = state.devices.read().await.len();
    Json(serde_json::json!({
        "ok": true,
        "service": "fatir-relay",
        "connected_devices": devices
    }))
}

async fn desktop_ws(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(token) = bearer(&headers) else {
        return (StatusCode::UNAUTHORIZED, "Missing relay token").into_response();
    };
    if !valid_device_id(&device_id) || token.len() < 32 {
        return (StatusCode::BAD_REQUEST, "Invalid device credentials").into_response();
    }

    {
        let devices = state.devices.read().await;
        if let Some(existing) = devices.get(&device_id) {
            if !constant_time_eq(existing.token.as_bytes(), token.as_bytes()) {
                return (StatusCode::UNAUTHORIZED, "Relay token mismatch").into_response();
            }
        }
    }

    ws.on_upgrade(move |socket| desktop_socket(state, device_id, token, socket))
}

async fn desktop_socket(state: AppState, device_id: String, token: String, socket: WebSocket) {
    let (mut socket_tx, mut socket_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let session_id = Uuid::new_v4().to_string();

    {
        let mut devices = state.devices.write().await;
        devices.insert(
            device_id.clone(),
            DeviceSession {
                token: Arc::new(token),
                tx: tx.clone(),
                session_id: session_id.clone(),
            },
        );
    }

    let writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if socket_tx.send(message).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = socket_rx.next().await {
        match message {
            Message::Text(text) => {
                if let Ok(parsed) = serde_json::from_str::<TunnelMessage>(&text) {
                    match parsed {
                        TunnelMessage::Response(response) => {
                            let sender = state.pending.write().await.remove(&response.id);
                            if let Some(sender) = sender {
                                let _ = sender.send(response);
                            }
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
    let mut devices = state.devices.write().await;
    if devices
        .get(&device_id)
        .map(|session| session.session_id.as_str())
        == Some(session_id.as_str())
    {
        devices.remove(&device_id);
    }
}

async fn proxy(
    State(state): State<AppState>,
    Path((device_id, path)): Path<(String, String)>,
    request: Request,
) -> Response {
    let Some(token) = bearer(request.headers()) else {
        return json_error(StatusCode::UNAUTHORIZED, "Missing relay token");
    };

    let session = {
        let devices = state.devices.read().await;
        devices.get(&device_id).cloned()
    };
    let Some(session) = session else {
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "Fatir desktop is offline");
    };
    if !constant_time_eq(session.token.as_bytes(), token.as_bytes()) {
        return json_error(StatusCode::UNAUTHORIZED, "Invalid relay token");
    }

    let method = request.method().to_string();
    let uri = request.uri().clone();
    let mut forwarded_path = format!("/{}", path);
    if let Some(query) = uri.query() {
        forwarded_path.push('?');
        forwarded_path.push_str(query);
    }

    if !forwarded_path.starts_with("/api/") {
        return json_error(StatusCode::NOT_FOUND, "Only Fatir API routes can be relayed");
    }

    let headers = selected_headers(request.headers());
    let body = match to_bytes(request.into_body(), state.max_body).await {
        Ok(body) => body,
        Err(_) => return json_error(StatusCode::PAYLOAD_TOO_LARGE, "Relay body limit exceeded"),
    };

    let id = Uuid::new_v4().to_string();
    let message = TunnelMessage::Request(TunnelRequest {
        id: id.clone(),
        method,
        path: forwarded_path,
        headers,
        body_b64: B64.encode(body),
    });

    let encoded = match serde_json::to_string(&message) {
        Ok(value) => value,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Unable to encode relay request"),
    };

    let (response_tx, response_rx) = oneshot::channel();
    state.pending.write().await.insert(id.clone(), response_tx);
    if session.tx.send(Message::Text(encoded)).is_err() {
        state.pending.write().await.remove(&id);
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "Fatir desktop disconnected");
    }

    let response = match tokio::time::timeout(RPC_TIMEOUT, response_rx).await {
        Ok(Ok(response)) => response,
        _ => {
            state.pending.write().await.remove(&id);
            return json_error(StatusCode::GATEWAY_TIMEOUT, "Fatir desktop task timed out");
        }
    };

    if let Some(error) = response.error {
        return json_error(StatusCode::BAD_GATEWAY, &error);
    }

    let bytes = match B64.decode(response.body_b64.as_bytes()) {
        Ok(bytes) => bytes,
        Err(_) => return json_error(StatusCode::BAD_GATEWAY, "Invalid desktop response payload"),
    };

    let mut output = Response::new(Body::from(bytes));
    *output.status_mut() = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
    for (name, value) in response.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            if is_safe_response_header(&name) {
                output.headers_mut().insert(name, value);
            }
        }
    }
    output
}

fn selected_headers(headers: &HeaderMap) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for name in [header::CONTENT_TYPE, header::ACCEPT, HeaderName::from_static("x-fatir-filename")] {
        if let Some(value) = headers.get(&name).and_then(|value| value.to_str().ok()) {
            out.insert(name.as_str().to_string(), value.to_string());
        }
    }
    out
}

fn is_safe_response_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "content-type" | "content-disposition" | "content-length"
    )
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn valid_device_id(value: &str) -> bool {
    value.len() >= 16
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}
