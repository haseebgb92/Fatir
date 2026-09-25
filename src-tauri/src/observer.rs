use crate::memory;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, fs, path::PathBuf, time::Duration};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverConfig {
    pub enabled: bool,
    pub active_apps: bool,
    pub browser_history: bool,
    pub terminal_history: bool,
    pub sample_seconds: u64,
}

impl Default for ObserverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            active_apps: true,
            browser_history: true,
            terminal_history: true,
            sample_seconds: 8,
        }
    }
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
}

fn config_path() -> PathBuf { data_dir().join("observer-config.json") }

pub fn load_config() -> ObserverConfig {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &ObserverConfig) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    fs::write(config_path(), serde_json::to_vec_pretty(config)?)?;
    Ok(())
}

pub fn status() -> Value {
    let config = load_config();
    let stats = memory::observation_stats().unwrap_or_else(|_| json!({"events":0}));
    json!({"config":config,"stats":stats})
}

fn redact_sensitive(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() { return String::new(); }
    let lower = trimmed.to_lowercase();
    let risky = [
        "password", "passwd", "api_key", "api-key", "apikey", "secret", "bearer ",
        "authorization:", "token=", "access_token", "refresh_token", "private_key",
        "otp", "one-time", "recovery code", "sk-", "ghp_", "github_pat_",
    ];
    if risky.iter().any(|needle| lower.contains(needle)) {
        return "[redacted sensitive activity]".into();
    }
    let mut s = trimmed.chars().take(420).collect::<String>();
    if trimmed.chars().count() > 420 { s.push('…'); }
    s
}

async fn active_window() -> Option<Value> {
    let id_out = Command::new("xdotool").args(["getactivewindow"]).output().await.ok()?;
    if !id_out.status.success() { return None; }
    let id = String::from_utf8_lossy(&id_out.stdout).trim().to_string();
    if id.is_empty() { return None; }

    let title_out = Command::new("xdotool").args(["getwindowname", &id]).output().await.ok()?;
    let title = redact_sensitive(&String::from_utf8_lossy(&title_out.stdout));

    let class_out = Command::new("xprop").args(["-id", &id, "WM_CLASS"]).output().await.ok();
    let class_text = class_out
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let app = class_text.split('=').nth(1).unwrap_or("").replace('"', "").split(',').last().unwrap_or("").trim().to_string();
    if app.to_lowercase().contains("fatir") || title == "Fatir" { return None; }
    let app_label = if app.is_empty() { "unknown".to_string() } else { app };
    Some(json!({"app":app_label, "title":title}))
}

fn shell_history_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    for p in [home.join(".bash_history"), home.join(".zsh_history")] {
        if p.exists() { return Some(p); }
    }
    None
}

fn parse_shell_line(line: &str) -> String {
    let raw = if line.starts_with(':') && line.contains(';') {
        line.split_once(';').map(|(_, v)| v).unwrap_or(line)
    } else { line };
    redact_sensitive(raw)
}

fn url_domain(value: &str) -> String {
    url::Url::parse(value)
        .ok()
        .and_then(|u| u.host_str().map(|x| x.to_string()))
        .unwrap_or_default()
}

async fn browser_history_snapshot() -> Vec<Value> {
    let Some(home) = dirs::home_dir() else { return Vec::new(); };
    let candidates = [
        home.join(".config/google-chrome/Default/History"),
        home.join(".config/chromium/Default/History"),
        home.join(".config/BraveSoftware/Brave-Browser/Default/History"),
        home.join(".config/microsoft-edge/Default/History"),
    ];
    let paths: Vec<String> = candidates.into_iter().filter(|p| p.exists()).map(|p| p.display().to_string()).collect();
    if paths.is_empty() { return Vec::new(); }

    let script = r#"
import json, os, shutil, sqlite3, sys, tempfile
from urllib.parse import urlsplit, urlunsplit
out=[]
for src in sys.argv[1:]:
    try:
        fd,tmp=tempfile.mkstemp(prefix='ah-history-',suffix='.sqlite')
        os.close(fd)
        shutil.copy2(src,tmp)
        con=sqlite3.connect(tmp)
        rows=con.execute('select url,title,last_visit_time from urls order by last_visit_time desc limit 25').fetchall()
        con.close(); os.unlink(tmp)
        for url,title,last_visit in rows:
            try:
                p=urlsplit(url)
                if p.scheme not in ('http','https'): continue
                clean=urlunsplit((p.scheme,p.netloc,p.path,'',''))
                out.append({'url':clean,'title':(title or '')[:220],'last_visit':last_visit})
            except Exception: pass
    except Exception:
        pass
print(json.dumps(out))
"#;
    let mut cmd = Command::new("python3");
    cmd.arg("-c").arg(script);
    for p in &paths { cmd.arg(p); }
    let Ok(output) = cmd.output().await else { return Vec::new(); };
    if !output.status.success() { return Vec::new(); }
    serde_json::from_slice::<Vec<Value>>(&output.stdout).unwrap_or_default()
}

pub async fn run_forever() {
    let mut last_window = String::new();
    let mut terminal_seen = 0usize;
    let mut web_seen: HashSet<String> = HashSet::new();
    let mut seeded_web = false;
    let mut tick = 0u64;

    if let Some(path) = shell_history_path() {
        terminal_seen = fs::read_to_string(path).map(|s| s.lines().count()).unwrap_or(0);
    }

    loop {
        let config = load_config();
        let delay = config.sample_seconds.clamp(5, 60);
        if config.enabled {
            if config.active_apps {
                if let Some(event) = active_window().await {
                    let signature = format!("{}|{}", event.get("app").and_then(Value::as_str).unwrap_or(""), event.get("title").and_then(Value::as_str).unwrap_or(""));
                    if !signature.is_empty() && signature != last_window {
                        let _ = memory::log_observation("active_window", &event);
                        last_window = signature;
                    }
                }
            }

            if config.terminal_history && tick % 2 == 0 {
                if let Some(path) = shell_history_path() {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let lines: Vec<&str> = content.lines().collect();
                        if lines.len() < terminal_seen { terminal_seen = 0; }
                        for line in lines.iter().skip(terminal_seen).take(80) {
                            let command = parse_shell_line(line);
                            if command.is_empty() || command == "[redacted sensitive activity]" { continue; }
                            let _ = memory::log_observation("terminal_command", &json!({"command":command}));
                        }
                        terminal_seen = lines.len();
                    }
                }
            }

            if config.browser_history && tick % 8 == 0 {
                let rows = browser_history_snapshot().await;
                let keys: Vec<String> = rows.iter().filter_map(|row| {
                    let url = row.get("url")?.as_str()?;
                    let last = row.get("last_visit").and_then(Value::as_i64).unwrap_or(0);
                    Some(format!("{last}|{url}"))
                }).collect();
                if !seeded_web {
                    for key in keys { web_seen.insert(key); }
                    seeded_web = true;
                } else {
                    for row in rows.into_iter().rev() {
                        let url = row.get("url").and_then(Value::as_str).unwrap_or("").to_string();
                        let last = row.get("last_visit").and_then(Value::as_i64).unwrap_or(0);
                        let key = format!("{last}|{url}");
                        if url.is_empty() || !web_seen.insert(key) { continue; }
                        let title = redact_sensitive(row.get("title").and_then(Value::as_str).unwrap_or(""));
                        let domain = url_domain(&url);
                        let _ = memory::log_observation("web_visit", &json!({"url":url,"domain":domain,"title":title}));
                    }
                    if web_seen.len() > 2000 {
                        web_seen = web_seen.into_iter().take(1000).collect();
                    }
                }
            }
        }
        tick = tick.wrapping_add(1);
        tokio::time::sleep(Duration::from_secs(delay)).await;
    }
}
