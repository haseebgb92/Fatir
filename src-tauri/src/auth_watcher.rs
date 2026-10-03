use crate::{credentials, desktop};
use crate::ollama::SharedState;
use serde_json::Value;
use std::{collections::HashMap, fs, time::{Duration, Instant}};

fn meta(row: &Value) -> String {
    format!(
        "{} {} {}",
        row.get("title").and_then(Value::as_str).unwrap_or(""),
        row.get("app").and_then(Value::as_str).unwrap_or(""),
        row.get("wm_class").and_then(Value::as_str).unwrap_or("")
    ).to_ascii_lowercase()
}

fn is_browser(s: &str) -> bool {
    ["chrome","chromium","firefox","brave","edge","vivaldi"]
        .iter().any(|x| s.contains(x))
}

fn is_terminal(s: &str) -> bool {
    ["terminal","gnome-terminal","xfce4-terminal","konsole","kitty","alacritty","xterm","tilix"]
        .iter().any(|x| s.contains(x))
}

fn protected_path(v: &Value) -> Option<String> {
    v.get("elements")?.as_array()?.iter().find_map(|e| {
        let states = e.get("states").and_then(Value::as_array)?;
        let protected = states.iter().filter_map(Value::as_str)
            .any(|s| s.eq_ignore_ascii_case("protected"));
        let role = e.get("role").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
        if protected && (role.contains("entry") || role.contains("text") || role.contains("password")) {
            e.get("path").and_then(Value::as_str).map(str::to_string)
        } else {
            None
        }
    })
}

fn element_text(v: &Value) -> String {
    v.get("elements").and_then(Value::as_array).map(|items| {
        items.iter().flat_map(|e| [
            e.get("name").and_then(Value::as_str).unwrap_or(""),
            e.get("description").and_then(Value::as_str).unwrap_or(""),
            e.get("role").and_then(Value::as_str).unwrap_or("")
        ]).collect::<Vec<_>>().join(" ").to_ascii_lowercase()
    }).unwrap_or_default()
}

fn looks_like_system_auth(window_meta: &str, text: &str) -> bool {
    let meta_match = [
        "polkit", "policykit", "pkexec", "authentication agent",
        "authentication required", "system authentication"
    ].iter().any(|x| window_meta.contains(x));

    let text_match = [
        "authentication is required",
        "authentication required",
        "administrator privileges",
        "administrative privileges",
        "authenticate to",
        "system policy",
        "root privileges"
    ].iter().any(|x| text.contains(x));

    meta_match || text_match
}


fn process_name(pid: i64) -> String {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn privilege_processes() -> (bool, bool) {
    let mut sudo=false;
    let mut pkexec=false;
    let Ok(entries)=fs::read_dir("/proc") else { return (false,false); };
    for entry in entries.flatten() {
        let Ok(pid)=entry.file_name().to_string_lossy().parse::<i64>() else { continue; };
        match process_name(pid).as_str() {
            "sudo" => sudo=true,
            "pkexec" => pkexec=true,
            _ => {}
        }
        if sudo && pkexec { break; }
    }
    (sudo,pkexec)
}


async fn active_window_identity() -> (i64, String) {
    let id = match tokio::process::Command::new("xdotool").args(["getactivewindow"]).output().await {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => return (0,String::new()),
    };
    if id.is_empty() { return (0,String::new()); }
    let pid = tokio::process::Command::new("xdotool").args(["getwindowpid", &id]).output().await
        .ok().filter(|o|o.status.success())
        .and_then(|o|String::from_utf8(o.stdout).ok())
        .and_then(|x|x.trim().parse::<i64>().ok()).unwrap_or(0);
    let title = tokio::process::Command::new("xdotool").args(["getwindowname", &id]).output().await
        .ok().filter(|o|o.status.success())
        .map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    (pid,title)
}

async fn resident_auth_enabled(state: &SharedState) -> bool {
    // Explicit Fatir approval gates still win over resident authentication.
    state.pending.lock().await.is_empty()
}

async fn handle_terminal_prompt(
    title: &str,
    handled: &mut HashMap<String, Instant>,
) -> bool {
    let Ok(v) = desktop::window_text(title).await else { return false; };
    let text = v.get("text").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();

    // The monitor already requires a real system sudo process and this exact
    // terminal to be the currently focused window. Require the actual sudo prompt
    // text as the final guard before using the stored credential.
    if !(text.contains("[sudo] password for") || text.contains("sudo password for")) {
        return false;
    }

    let key = format!("terminal:{title}");
    if handled.get(&key).map(|t| t.elapsed() < Duration::from_secs(120)).unwrap_or(false) {
        return false;
    }

    let Ok(secret) = credentials::sudo_secret() else { return false; };
    let result = desktop::type_secret_to_window(title, &secret).await;
    drop(secret);

    if result.is_ok() {
        handled.insert(key, Instant::now());
        true
    } else {
        false
    }
}

async fn handle_system_auth(
    title: &str,
    window_meta: &str,
    handled: &mut HashMap<String, Instant>,
) -> bool {
    let Ok(elements) = desktop::elements(title).await else { return false; };
    let text = element_text(&elements);

    if !looks_like_system_auth(window_meta, &text) {
        return false;
    }

    let Some(path) = protected_path(&elements) else { return false; };
    let key = format!("auth:{title}:{path}");
    if handled.get(&key).map(|t| t.elapsed() < Duration::from_secs(120)).unwrap_or(false) {
        return false;
    }

    let Ok(secret) = credentials::sudo_secret() else { return false; };
    let filled = desktop::set_secret_text(title, &path, &secret).await.is_ok();
    drop(secret);
    if !filled { return false; }

    // Submit only through an unambiguous authentication control.
    for label in ["Authenticate", "Unlock", "OK", "Continue"] {
        if desktop::activate_named(title, label, Some("push button"), 0).await.is_ok() {
            handled.insert(key, Instant::now());
            return true;
        }
    }

    // Keep the rate-limit even when only the field could be filled.
    handled.insert(key, Instant::now());
    true
}

pub async fn monitor_forever(state: SharedState) {
    let mut handled: HashMap<String, Instant> = HashMap::new();

    loop {
        handled.retain(|_, t| t.elapsed() < Duration::from_secs(600));

        if credentials::has_sudo() && resident_auth_enabled(&state).await {
            // Cheap /proc probe first. Full AT-SPI/X11 enumeration is relatively
            // expensive, so only wake the desktop scanner while a real privilege
            // process is present.
            let (sudo_active, system_auth_active) = privilege_processes();

            if sudo_active || system_auth_active {
                let (active_pid,active_title)=if sudo_active { active_window_identity().await } else {(0,String::new())};
                if let Ok(v) = desktop::windows().await {
                    if let Some(rows) = v.get("windows").and_then(Value::as_array) {
                        for row in rows {
                            let title = row.get("title").and_then(Value::as_str).unwrap_or("").trim();
                            if title.is_empty() { continue; }

                            let window_meta = meta(row);
                            if is_browser(&window_meta) { continue; }

                            if sudo_active && is_terminal(&window_meta) {
                                let pid=row.get("pid").and_then(Value::as_i64).unwrap_or(0);
                                let focused=(active_pid>0 && pid==active_pid) || (!active_title.is_empty() && title==active_title);
                                if focused { let _ = handle_terminal_prompt(title, &mut handled).await; }
                            } else if system_auth_active && !is_terminal(&window_meta) {
                                let _ = handle_system_auth(title, &window_meta, &mut handled).await;
                            }
                        }
                    }
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub fn status() -> Value {
    serde_json::json!({
        "enabled": true,
        "scan_seconds": 2,
        "scope": "resident local terminal/system authentication",
        "terminal_sudo_prompts": true,
        "requires_real_sudo_process": true,
        "requires_focused_terminal": true,
        "system_auth_dialogs": true,
        "browser_password_fields": false,
        "pending_approval_pauses": true,
        "prompt_retry_cooldown_seconds": 120
    })
}
