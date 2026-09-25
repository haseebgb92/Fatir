use crate::ollama::SharedState;
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMeta {
    session_id: String,
    title: String,
    created_at: String,
    updated_at: String,
    source: String,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
}

fn meta_path() -> PathBuf { data_dir().join("chat-index.json") }

fn registry_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn load_meta() -> Vec<ChatMeta> {
    fs::read_to_string(meta_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_meta(items: &[ChatMeta]) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let tmp = data_dir().join("chat-index.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(items)?)?;
    fs::rename(tmp, meta_path())?;
    Ok(())
}

fn clean_title(text: &str) -> String {
    let compact = text
        .replace('\r', " ")
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if compact.is_empty() {
        return "New chat".into();
    }
    compact.chars().take(72).collect()
}

fn source_for(session_id: &str) -> String {
    if session_id.starts_with("schedule-") { "schedule".into() }
    else if session_id.starts_with("companion-") { "companion".into() }
    else { "desktop".into() }
}

pub fn touch(session_id: &str, user_text: &str) -> Result<()> {
    let _guard = registry_lock().lock().map_err(|_| anyhow!("Chat registry lock poisoned"))?;
    let mut items = load_meta();
    let now = Utc::now().to_rfc3339();
    if let Some(item) = items.iter_mut().find(|x| x.session_id == session_id) {
        item.updated_at = now;
        if item.title.trim().is_empty() || item.title == "New chat" {
            item.title = clean_title(user_text);
        }
    } else {
        items.push(ChatMeta {
            session_id: session_id.to_string(),
            title: clean_title(user_text),
            created_at: now.clone(),
            updated_at: now,
            source: source_for(session_id),
        });
    }
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    save_meta(&items)
}

fn message_text(message: &Value) -> Option<String> {
    message.get("content").and_then(Value::as_str).map(str::to_string)
}

fn visible_messages(messages: &[Value]) -> Vec<Value> {
    messages
        .iter()
        .filter_map(|m| {
            let role = m.get("role").and_then(Value::as_str)?;
            if !matches!(role, "user" | "assistant") { return None; }
            let content = message_text(m)?;
            if content.trim().is_empty() { return None; }
            Some(json!({
                "role": role,
                "content": content
            }))
        })
        .collect()
}

pub async fn list(state: &SharedState) -> Result<Vec<Value>> {
    let sessions = state.sessions.lock().await.clone();
    let meta = load_meta();
    let meta_by_id: HashMap<String, ChatMeta> =
        meta.into_iter().map(|x| (x.session_id.clone(), x)).collect();

    let mut rows = Vec::new();
    for (session_id, messages) in sessions {
        let visible = visible_messages(&messages);
        if visible.is_empty() { continue; }
        let first_user = visible.iter()
            .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))
            .and_then(|m| m.get("content").and_then(Value::as_str))
            .unwrap_or("New chat");
        let preview = visible.last()
            .and_then(|m| m.get("content").and_then(Value::as_str))
            .unwrap_or("");
        let meta = meta_by_id.get(&session_id);
        let source = meta.map(|x| x.source.clone()).unwrap_or_else(|| source_for(&session_id));
        rows.push(json!({
            "session_id": session_id.clone(),
            "title": meta.map(|x| x.title.clone()).unwrap_or_else(|| clean_title(first_user)),
            "preview": preview.chars().take(180).collect::<String>(),
            "message_count": visible.len(),
            "created_at": meta.map(|x| x.created_at.clone()),
            "updated_at": meta.map(|x| x.updated_at.clone()),
            "source": source,
        }));
    }

    rows.sort_by(|a, b| {
        let aa = a.get("updated_at").and_then(Value::as_str).unwrap_or("");
        let bb = b.get("updated_at").and_then(Value::as_str).unwrap_or("");
        bb.cmp(aa)
    });
    Ok(rows)
}

pub async fn history(state: &SharedState, session_id: &str) -> Result<Value> {
    let sessions = state.sessions.lock().await;
    let messages = sessions.get(session_id).ok_or_else(|| anyhow!("Chat not found"))?;
    Ok(json!({
        "session_id": session_id,
        "messages": visible_messages(messages)
    }))
}

pub async fn remove(state: &SharedState, session_id: &str) -> Result<Value> {
    {
        let mut sessions = state.sessions.lock().await;
        sessions.remove(session_id).ok_or_else(|| anyhow!("Chat not found"))?;
        crate::memory::save_sessions(&sessions)?;
    }
    let _guard = registry_lock().lock().map_err(|_| anyhow!("Chat registry lock poisoned"))?;
    let mut items = load_meta();
    items.retain(|x| x.session_id != session_id);
    save_meta(&items)?;
    Ok(json!({"removed":session_id}))
}

pub fn known_session_ids() -> HashSet<String> {
    load_meta().into_iter().map(|x| x.session_id).collect()
}
