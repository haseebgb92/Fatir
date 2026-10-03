use crate::{ollama, tasks};
use crate::ollama::SharedState;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, time::Duration};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Continuation {
    pub task_id: String,
    pub session_id: String,
    pub mode: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub consecutive_failures: u32,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
}
fn path() -> PathBuf { data_dir().join("continuations.json") }

fn load() -> Vec<Continuation> {
    fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
fn save(items: &[Continuation]) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let tmp = data_dir().join("continuations.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(items)?)?;
    fs::rename(tmp, path())?;
    Ok(())
}

pub fn bind(task_id: &str, session_id: &str, mode: &str) -> Result<()> {
    let mut items = load();
    let now = Utc::now().to_rfc3339();
    if let Some(x) = items.iter_mut().find(|x| x.task_id == task_id) {
        x.session_id = session_id.to_string();
        x.mode = mode.to_string();
        x.updated_at = now;
    } else {
        items.push(Continuation {
            task_id: task_id.to_string(),
            session_id: session_id.to_string(),
            mode: if mode.trim().is_empty() { "auto".into() } else { mode.to_string() },
            created_at: now.clone(),
            updated_at: now,
            last_run_at: None,
            consecutive_failures: 0,
        });
    }
    items.truncate(100);
    save(&items)
}

fn terminal_status(v: &Value) -> &str {
    v.get("status").and_then(Value::as_str).unwrap_or("")
}

fn due(item: &Continuation, task: &Value) -> bool {
    let interval = match terminal_status(task) {
        "active" => 45,
        "waiting" => 45,
        "blocked" => 300,
        _ => return false,
    };
    let Some(last) = item.last_run_at.as_deref() else { return true; };
    let Ok(parsed) = DateTime::parse_from_rfc3339(last) else { return true; };
    Utc::now().signed_duration_since(parsed.with_timezone(&Utc)).num_seconds() >= interval
}

async fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send").args([title, body]).status().await;
}

pub async fn run_once(state: SharedState) -> Result<Value> {
    let mut items = load();
    let mut attempted = 0u64;
    let mut skipped = 0u64;
    let mut completed = Vec::new();

    for i in 0..items.len() {
        let task_id = items[i].task_id.clone();
        let session_id = items[i].session_id.clone();
        let mode = items[i].mode.clone();
        let task = match tasks::get(&task_id) {
            Ok(v) => v,
            Err(_) => { completed.push(task_id); continue; }
        };
        let status = terminal_status(&task).to_string();
        if matches!(status.as_str(), "completed" | "cancelled") {
            completed.push(task_id);
            continue;
        }
        if !due(&items[i], &task) { skipped += 1; continue; }

        // Never create a second agent run for a session already being handled.
        if state.runs.lock().await.contains_key(&session_id) { skipped += 1; continue; }

        // A pending approval is a genuine human gate. Do not keep re-entering the model.
        if state.pending.lock().await.values().any(|p| p.session_id == session_id) {
            skipped += 1;
            continue;
        }

        attempted += 1;
        items[i].last_run_at = Some(Utc::now().to_rfc3339());
        items[i].updated_at = Utc::now().to_rfc3339();
        save(&items)?;

        ollama::refresh_sessions_from_disk(&state).await;

        let instruction = format!(
            "AUTONOMOUS CONTINUATION for persistent task {task_id}. Re-check the current PC/browser/terminal state and continue working toward the task objective without waiting for another user message. Do not merely summarize progress. Use existing task state and checkpoints. If something is still processing, wait/poll or schedule the appropriate next check. If an approval is required, stop at that approval. If a real blocker remains after recovery attempts, record it. Only complete the task after concrete verification."
        );

        match ollama::send_message(state.clone(), &session_id, &instruction, Vec::new(), &mode).await {
            Ok(_) => {
                items[i].consecutive_failures = 0;
                if let Ok(after) = tasks::get(&task_id) {
                    match terminal_status(&after) {
                        "completed" => {
                            let title = after.get("title").and_then(Value::as_str).unwrap_or("Fatir task");
                            notify("Fatir task completed", title).await;
                            completed.push(task_id.clone());
                        }
                        "blocked" => {
                            let title = after.get("title").and_then(Value::as_str).unwrap_or("Fatir task");
                            let err = after.get("last_error").and_then(Value::as_str).unwrap_or("Task needs attention");
                            notify("Fatir task needs attention", &format!("{title}: {err}")).await;
                        }
                        _ => {}
                    }
                }
            }
            Err(err) => {
                items[i].consecutive_failures = items[i].consecutive_failures.saturating_add(1);
                if items[i].consecutive_failures >= 3 {
                    let _ = tasks::record_error(&task_id, &format!("Background continuation failed repeatedly: {err}"));
                    notify("Fatir task paused", "A persistent task hit repeated continuation errors and needs attention.").await;
                }
            }
        }
        items[i].updated_at = Utc::now().to_rfc3339();
        save(&items)?;
    }

    if !completed.is_empty() {
        items.retain(|x| !completed.iter().any(|id| id == &x.task_id));
        save(&items)?;
    }

    Ok(json!({
        "bound_tasks": items.len(),
        "attempted": attempted,
        "skipped": skipped
    }))
}

pub async fn monitor_forever(state: SharedState) {
    loop {
        let _ = run_once(state.clone()).await;
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
}

pub fn status() -> Value {
    let items = load();
    json!({
        "enabled": true,
        "bound_tasks": items.len(),
        "tasks": items.into_iter().map(|x| json!({
            "task_id": x.task_id,
            "session_id": x.session_id,
            "last_run_at": x.last_run_at,
            "consecutive_failures": x.consecutive_failures
        })).collect::<Vec<_>>()
    })
}
