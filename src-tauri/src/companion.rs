use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    sync::Arc,
    time::UNIX_EPOCH,
};
use tokio::{fs::File, io::AsyncWriteExt, net::TcpListener};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{
    agent_schedules,
    chats,
    models::Attachment,
    ollama::{self, SharedState},
};

pub const COMPANION_PORT: u16 = 32145;
const MAX_UPLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct CompanionInfo {
    pub enabled: bool,
    pub device_name: String,
    pub address: String,
    pub port: u16,
    pub base_url: String,
    pub token: String,
    pub roots: Vec<String>,
}

#[derive(Clone)]
pub struct CompanionService {
    shared: SharedState,
    token: Arc<String>,
    roots: Arc<Vec<PathBuf>>,
    upload_dir: Arc<PathBuf>,
    info: CompanionInfo,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    name: &'static str,
    version: &'static str,
    device_name: String,
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    #[serde(default)]
    session_id: Option<String>,
    text: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    attachments: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ActionRequest {
    action_id: String,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PathQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    directory: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatHistoryQuery {
    session_id: String,
}

#[derive(Debug, Deserialize)]
struct IdRequest {
    id: String,
}

#[derive(Debug, Serialize)]
struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified_unix: Option<u64>,
    mime: Option<String>,
}

impl CompanionService {
    pub fn new(shared: SharedState) -> Self {
        let base_data = dirs::data_local_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
            .join("Fatir")
            .join("companion");
        let _ = fs::create_dir_all(&base_data);

        let token = load_or_create_token(&base_data);
        let upload_dir = base_data.join("uploads");
        let _ = fs::create_dir_all(&upload_dir);

        let mut roots = Vec::<PathBuf>::new();
        if let Some(home) = dirs::home_dir() {
            roots.push(home.clone());
            if let Some(user) = home.file_name().and_then(|v| v.to_str()) {
                let media = PathBuf::from("/media").join(user);
                if media.exists() {
                    roots.push(media);
                }
            }
        }
        roots.push(upload_dir.clone());

        let roots = roots
            .into_iter()
            .filter_map(|p| fs::canonicalize(&p).ok().or(Some(p)))
            .fold(Vec::<PathBuf>::new(), |mut acc, p| {
                if !acc.iter().any(|existing| p.starts_with(existing)) {
                    acc.push(p);
                }
                acc
            });

        let ip = local_lan_ip().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let device_name = hostname();
        let address = ip.to_string();
        let base_url = format!("http://{}:{}", address, COMPANION_PORT);

        let info = CompanionInfo {
            enabled: true,
            device_name,
            address,
            port: COMPANION_PORT,
            base_url,
            token: token.clone(),
            roots: roots.iter().map(|p| p.display().to_string()).collect(),
        };

        Self {
            shared,
            token: Arc::new(token),
            roots: Arc::new(roots),
            upload_dir: Arc::new(upload_dir),
            info,
        }
    }

    pub fn info(&self) -> CompanionInfo {
        self.info.clone()
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        let router = Router::new()
            .route("/api/v1/health", get(health))
            .route("/api/v1/chat", post(chat))
            .route("/api/v1/approve", post(approve))
            .route("/api/v1/deny", post(deny))
            .route("/api/v1/chats", get(chat_list))
            .route("/api/v1/chats/history", get(chat_history))
            .route("/api/v1/agent-schedules", get(agent_schedule_list))
            .route("/api/v1/agent-schedules/run", post(agent_schedule_run))
            .route("/api/v1/agent-schedules/cancel", post(agent_schedule_cancel))
            .route("/api/v1/files/roots", get(file_roots))
            .route("/api/v1/files/list", get(list_files))
            .route("/api/v1/files/download", get(download_file))
            .route("/api/v1/files/upload", post(upload_file))
            .with_state(self);

        let listener = TcpListener::bind(("0.0.0.0", COMPANION_PORT)).await?;
        axum::serve(listener, router).await?;
        Ok(())
    }

    fn is_allowed(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| path.starts_with(root))
    }

    fn resolve_existing(&self, raw: &str) -> Result<PathBuf, String> {
        let expanded = expand_home(raw);
        let canonical = fs::canonicalize(&expanded).map_err(|e| e.to_string())?;
        if !self.is_allowed(&canonical) {
            return Err("Path is outside Companion-approved Linux roots".into());
        }
        Ok(canonical)
    }

    fn resolve_directory(&self, raw: Option<&str>) -> Result<PathBuf, String> {
        if let Some(raw) = raw.filter(|v| !v.trim().is_empty()) {
            let path = self.resolve_existing(raw)?;
            if !path.is_dir() {
                return Err("Destination is not a directory".into());
            }
            return Ok(path);
        }
        Ok((*self.upload_dir).clone())
    }
}

async fn health(State(state): State<CompanionService>) -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        name: "Fatir Companion",
        version: env!("CARGO_PKG_VERSION"),
        device_name: state.info.device_name.clone(),
    })
}

async fn chat(
    State(state): State<CompanionService>,
    headers: HeaderMap,
    Json(request): Json<ChatRequest>,
) -> Response {
    if let Err(response) = require_auth(&headers, &state) {
        return response;
    }
    if request.text.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "Message cannot be empty");
    }

    let mut attachments = Vec::new();
    for raw in request.attachments {
        match state.resolve_existing(&raw) {
            Ok(path) if path.is_file() => attachments.push(Attachment {
                path: path.display().to_string(),
            }),
            Ok(_) => return error(StatusCode::BAD_REQUEST, "Attachments must be files"),
            Err(e) => return error(StatusCode::FORBIDDEN, &e),
        }
    }

    let session_id = request
        .session_id
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| format!("companion-{}", Uuid::new_v4()));
    let mode = request.mode.unwrap_or_else(|| "auto".into());

    match ollama::send_message(
        state.shared.clone(),
        &session_id,
        &request.text,
        attachments,
        &mode,
    )
    .await
    {
        Ok(result) => Json(serde_json::json!({
            "session_id": session_id,
            "response": result
        }))
        .into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn approve(
    State(state): State<CompanionService>,
    headers: HeaderMap,
    Json(request): Json<ActionRequest>,
) -> Response {
    if let Err(response) = require_auth(&headers, &state) {
        return response;
    }
    let mode = request.mode.unwrap_or_else(|| "auto".into());
    match ollama::approve_action(state.shared.clone(), &request.action_id, &mode).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn deny(
    State(state): State<CompanionService>,
    headers: HeaderMap,
    Json(request): Json<ActionRequest>,
) -> Response {
    if let Err(response) = require_auth(&headers, &state) {
        return response;
    }
    let mode = request.mode.unwrap_or_else(|| "auto".into());
    match ollama::deny_action(state.shared.clone(), &request.action_id, &mode).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}


async fn chat_list(State(state): State<CompanionService>, headers: HeaderMap) -> Response {
    if let Err(response) = require_auth(&headers, &state) { return response; }
    match chats::list(&state.shared).await {
        Ok(rows) => Json(serde_json::json!({"chats":rows})).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn chat_history(
    State(state): State<CompanionService>,
    headers: HeaderMap,
    Query(query): Query<ChatHistoryQuery>,
) -> Response {
    if let Err(response) = require_auth(&headers, &state) { return response; }
    match chats::history(&state.shared, &query.session_id).await {
        Ok(row) => Json(row).into_response(),
        Err(e) => error(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn agent_schedule_list(State(state): State<CompanionService>, headers: HeaderMap) -> Response {
    if let Err(response) = require_auth(&headers, &state) { return response; }
    match agent_schedules::list() {
        Ok(rows) => Json(serde_json::json!({"schedules":rows})).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn agent_schedule_run(
    State(state): State<CompanionService>,
    headers: HeaderMap,
    Json(request): Json<IdRequest>,
) -> Response {
    if let Err(response) = require_auth(&headers, &state) { return response; }
    match agent_schedules::run_now(&request.id) {
        Ok(row) => Json(row).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn agent_schedule_cancel(
    State(state): State<CompanionService>,
    headers: HeaderMap,
    Json(request): Json<IdRequest>,
) -> Response {
    if let Err(response) = require_auth(&headers, &state) { return response; }
    match agent_schedules::cancel(&request.id) {
        Ok(row) => Json(row).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn file_roots(State(state): State<CompanionService>, headers: HeaderMap) -> Response {
    if let Err(response) = require_auth(&headers, &state) {
        return response;
    }
    Json(serde_json::json!({
        "roots": state.roots.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()
    }))
    .into_response()
}

async fn list_files(
    State(state): State<CompanionService>,
    headers: HeaderMap,
    Query(query): Query<PathQuery>,
) -> Response {
    if let Err(response) = require_auth(&headers, &state) {
        return response;
    }

    let directory = match query.path.as_deref() {
        Some(path) if !path.trim().is_empty() => match state.resolve_existing(path) {
            Ok(path) => path,
            Err(e) => return error(StatusCode::FORBIDDEN, &e),
        },
        _ => {
            return Json(serde_json::json!({
                "path": null,
                "entries": state.roots.iter().map(|root| FileEntry {
                    name: root.file_name().and_then(|v| v.to_str()).unwrap_or("/").to_string(),
                    path: root.display().to_string(),
                    is_dir: true,
                    size: 0,
                    modified_unix: None,
                    mime: None,
                }).collect::<Vec<_>>()
            })).into_response();
        }
    };

    if !directory.is_dir() {
        return error(StatusCode::BAD_REQUEST, "Path is not a directory");
    }

    let mut entries = Vec::new();
    let read = match fs::read_dir(&directory) {
        Ok(read) => read,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    for entry in read.flatten() {
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(v) => v,
            Err(_) => continue,
        };
        entries.push(FileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: path.display().to_string(),
            is_dir: metadata.is_dir(),
            size: if metadata.is_file() { metadata.len() } else { 0 },
            modified_unix: metadata
                .modified()
                .ok()
                .and_then(|v| v.duration_since(UNIX_EPOCH).ok())
                .map(|v| v.as_secs()),
            mime: if metadata.is_file() {
                mime_guess::from_path(&path).first_raw().map(str::to_string)
            } else {
                None
            },
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Json(serde_json::json!({
        "path": directory.display().to_string(),
        "entries": entries
    }))
    .into_response()
}

async fn download_file(
    State(state): State<CompanionService>,
    headers: HeaderMap,
    Query(query): Query<PathQuery>,
) -> Response {
    if let Err(response) = require_auth(&headers, &state) {
        return response;
    }
    let Some(raw) = query.path.as_deref() else {
        return error(StatusCode::BAD_REQUEST, "Missing path");
    };
    let path = match state.resolve_existing(raw) {
        Ok(path) if path.is_file() => path,
        Ok(_) => return error(StatusCode::BAD_REQUEST, "Path is not a file"),
        Err(e) => return error(StatusCode::FORBIDDEN, &e),
    };

    let file = match File::open(&path).await {
        Ok(file) => file,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    let filename = path.file_name().and_then(|v| v.to_str()).unwrap_or("download");
    let mime = mime_guess::from_path(&path).first_or_octet_stream();

    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", sanitize_filename(filename))) {
        response.headers_mut().insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

async fn upload_file(
    State(state): State<CompanionService>,
    headers: HeaderMap,
    Query(query): Query<UploadQuery>,
    body: Body,
) -> Response {
    if let Err(response) = require_auth(&headers, &state) {
        return response;
    }

    let filename = match headers
        .get("x-fatir-filename")
        .and_then(|v| v.to_str().ok())
        .map(sanitize_filename)
        .filter(|v| !v.is_empty())
    {
        Some(v) => v,
        None => return error(StatusCode::BAD_REQUEST, "Missing X-Fatir-Filename header"),
    };

    let directory = match state.resolve_directory(query.directory.as_deref()) {
        Ok(path) => path,
        Err(e) => return error(StatusCode::FORBIDDEN, &e),
    };
    let destination = unique_destination(&directory, &filename);

    let mut file = match File::create(&destination).await {
        Ok(file) => file,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let mut total = 0_u64;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                let _ = tokio::fs::remove_file(&destination).await;
                return error(StatusCode::BAD_REQUEST, &e.to_string());
            }
        };
        total = total.saturating_add(chunk.len() as u64);
        if total > MAX_UPLOAD_BYTES {
            let _ = tokio::fs::remove_file(&destination).await;
            return error(StatusCode::PAYLOAD_TOO_LARGE, "Upload exceeds the 2 GiB local companion limit");
        }
        if let Err(e) = file.write_all(&chunk).await {
            let _ = tokio::fs::remove_file(&destination).await;
            return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
        }
    }
    let _ = file.flush().await;

    Json(serde_json::json!({
        "ok": true,
        "path": destination.display().to_string(),
        "bytes": total,
        "name": destination.file_name().and_then(|v| v.to_str()).unwrap_or(&filename)
    }))
    .into_response()
}

fn require_auth(headers: &HeaderMap, state: &CompanionService) -> Result<(), Response> {
    let expected = format!("Bearer {}", state.token);
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if constant_time_eq(supplied.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(error(StatusCode::UNAUTHORIZED, "Invalid companion token"))
    }
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn expand_home(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_default().join(rest);
    }
    PathBuf::from(raw)
}

fn unique_destination(directory: &Path, filename: &str) -> PathBuf {
    let requested = directory.join(filename);
    if !requested.exists() {
        return requested;
    }

    let source = Path::new(filename);
    let stem = source.file_stem().and_then(|v| v.to_str()).unwrap_or("file");
    let ext = source.extension().and_then(|v| v.to_str());
    for index in 1..10_000 {
        let candidate_name = match ext {
            Some(ext) => format!("{} ({index}).{}", stem, ext),
            None => format!("{} ({index})", stem),
        };
        let candidate = directory.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{}-{}", Uuid::new_v4(), filename))
}

fn sanitize_filename(raw: &str) -> String {
    raw.chars()
        .filter(|c| !matches!(c, '/' | '\\' | '\0' | '\r' | '\n'))
        .collect::<String>()
        .trim()
        .chars()
        .take(180)
        .collect()
}

fn load_or_create_token(base: &Path) -> String {
    let token_path = base.join("device-token");
    if let Ok(value) = fs::read_to_string(&token_path) {
        let value = value.trim();
        if value.len() >= 32 {
            return value.to_string();
        }
    }

    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&token_path)
        {
            let _ = writeln!(file, "{token}");
        }
    }
    #[cfg(not(unix))]
    {
        let _ = fs::write(&token_path, &token);
    }
    token
}

fn local_lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            fs::read_to_string("/etc/hostname")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
        .unwrap_or_else(|| "Linux PC".into())
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
