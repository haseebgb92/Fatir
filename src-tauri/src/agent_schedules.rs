use crate::{
    chats,
    credentials,
    models::AgentResponse,
    ollama::{self, SharedState},
};
use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    process::Command,
    sync::{Mutex, OnceLock},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSchedule {
    pub id: String,
    pub label: String,
    pub prompt: String,
    pub trigger_kind: String,
    pub trigger: String,
    pub calendar: String,
    pub execution_mode: String,
    #[serde(default)]
    pub credential_ids: Vec<String>,
    pub verification_required: bool,
    pub unit: String,
    pub session_id: String,
    pub created_at: String,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub last_status: Option<String>,
    #[serde(default)]
    pub last_result: Option<String>,
    #[serde(default)]
    pub last_model: Option<String>,
    #[serde(default)]
    pub last_pending_action: Option<String>,
}

#[derive(Clone)]
struct ScheduleAuthorization {
    schedule_id: String,
    credential_ids: HashSet<String>,
}

tokio::task_local! {
    static SCHEDULE_AUTH: ScheduleAuthorization;
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
}

fn path() -> PathBuf { data_dir().join("agent-schedules.json") }
fn run_log_path() -> PathBuf { data_dir().join("agent-schedule-runs.jsonl") }
fn unit_dir() -> PathBuf { dirs::home_dir().unwrap_or_default().join(".config/systemd/user") }

fn registry_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn load() -> Vec<AgentSchedule> {
    fs::read_to_string(path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(items: &[AgentSchedule]) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let tmp = data_dir().join("agent-schedules.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(items)?)?;
    fs::rename(tmp, path())?;
    Ok(())
}

fn clean_label(value: &str) -> String {
    value.replace('\r', " ").replace('\n', " ").trim().chars().take(120).collect()
}

fn unit_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn parse_delay_seconds(value: &str) -> Result<i64> {
    let raw = value.trim().to_ascii_lowercase();
    if raw.is_empty() { return Err(anyhow!("Delay cannot be empty")); }
    let compact = raw.replace(' ', "");
    let split = compact.find(|c: char| !c.is_ascii_digit()).unwrap_or(compact.len());
    let (digits, unit) = compact.split_at(split);
    let amount: i64 = digits.parse().map_err(|_| anyhow!("Delay must look like 30s, 10m, 2h or 1d"))?;
    if amount <= 0 { return Err(anyhow!("Delay must be greater than zero")); }
    let multiplier = match unit {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
        "d" | "day" | "days" => 86400,
        _ => return Err(anyhow!("Unsupported delay unit; use seconds, minutes, hours or days")),
    };
    amount.checked_mul(multiplier).ok_or_else(|| anyhow!("Delay is too large"))
}

fn run_systemctl(args: &[&str]) -> Result<()> {
    let out = Command::new("systemctl")
        .args(["--user"])
        .args(args)
        .output()
        .context("Could not run systemctl --user")?;
    if !out.status.success() {
        return Err(anyhow!(
            "systemctl --user failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

fn validate_execution_mode(mode: &str) -> Result<()> {
    if matches!(mode, "agent" | "managed_browser" | "active_browser" | "headless_browser") {
        Ok(())
    } else {
        Err(anyhow!("execution_mode must be agent, managed_browser, active_browser, or headless_browser"))
    }
}

fn validate_credentials(ids: &[String]) -> Result<()> {
    if ids.is_empty() { return Ok(()); }
    let known: HashSet<String> = credentials::list()?
        .into_iter()
        .filter_map(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    for id in ids {
        if !known.contains(id) {
            return Err(anyhow!("Unknown Fatir credential id: {id}"));
        }
    }
    Ok(())
}

fn write_units(item: &AgentSchedule) -> Result<(PathBuf, PathBuf)> {
    fs::create_dir_all(unit_dir())?;
    let exe = std::env::current_exe().context("Could not determine Fatir executable path")?;
    let service = unit_dir().join(format!("{}.service", item.unit));
    let timer = unit_dir().join(format!("{}.timer", item.unit));
    let description = clean_label(&item.label);

    let service_text = format!(
        "[Unit]\nDescription=Fatir scheduled agent: {}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nExecStart={} --background --run-agent-schedule {}\n",
        description,
        unit_quote(&exe.display().to_string()),
        item.id
    );
    let timer_text = format!(
        "[Unit]\nDescription=Fatir agent timer: {}\n\n[Timer]\nOnCalendar={}\nPersistent=true\nUnit={}.service\n\n[Install]\nWantedBy=timers.target\n",
        description,
        item.calendar,
        item.unit
    );
    fs::write(&service, service_text)?;
    fs::write(&timer, timer_text)?;
    Ok((service, timer))
}

pub fn create(
    label: &str,
    prompt: &str,
    trigger_kind: &str,
    trigger: &str,
    execution_mode: &str,
    credential_ids: &[String],
    verification_required: bool,
) -> Result<Value> {
    if label.trim().is_empty() { return Err(anyhow!("Schedule label cannot be empty")); }
    if prompt.trim().is_empty() { return Err(anyhow!("Scheduled task prompt cannot be empty")); }
    if !matches!(trigger_kind, "delay" | "calendar") {
        return Err(anyhow!("trigger_kind must be delay or calendar"));
    }
    if trigger.trim().is_empty() || trigger.contains('\n') || trigger.contains('\r') {
        return Err(anyhow!("Schedule trigger must be one non-empty line"));
    }
    validate_execution_mode(execution_mode)?;
    validate_credentials(credential_ids)?;
    if execution_mode == "headless_browser" && !credential_ids.is_empty() {
        return Err(anyhow!(
            "Stored credentials are intentionally disabled for headless scheduled browser runs. Use managed_browser for authenticated schedules so Fatir can inject broker secrets into the correct isolated visible browser profile."
        ));
    }

    let calendar = if trigger_kind == "delay" {
        let seconds = parse_delay_seconds(trigger)?;
        let when = Local::now() + ChronoDuration::seconds(seconds);
        when.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        trigger.trim().to_string()
    };

    let id = Uuid::new_v4().to_string();
    let unit = format!("fatir-agent-schedule-{}", &id[..8]);
    let item = AgentSchedule {
        id: id.clone(),
        label: clean_label(label),
        prompt: prompt.trim().chars().take(20_000).collect(),
        trigger_kind: trigger_kind.into(),
        trigger: trigger.trim().chars().take(240).collect(),
        calendar: calendar.clone(),
        execution_mode: execution_mode.into(),
        credential_ids: credential_ids.to_vec(),
        verification_required,
        unit: unit.clone(),
        session_id: format!("schedule-{id}"),
        created_at: Utc::now().to_rfc3339(),
        last_run_at: None,
        last_status: Some("scheduled".into()),
        last_result: None,
        last_model: None,
        last_pending_action: None,
    };

    let (service, timer) = write_units(&item)?;
    let timer_name = format!("{}.timer", unit);
    if let Err(err) = run_systemctl(&["daemon-reload"])
        .and_then(|_| run_systemctl(&["enable", "--now", timer_name.as_str()]))
    {
        let _ = fs::remove_file(&timer);
        let _ = fs::remove_file(&service);
        let _ = run_systemctl(&["daemon-reload"]);
        return Err(err);
    }

    let _guard = registry_lock().lock().map_err(|_| anyhow!("Schedule registry lock poisoned"))?;
    let mut items = load();
    items.push(item.clone());
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    if let Err(err) = save(&items) {
        let _ = Command::new("systemctl").args(["--user", "disable", "--now", timer_name.as_str()]).status();
        let _ = fs::remove_file(&timer);
        let _ = fs::remove_file(&service);
        let _ = run_systemctl(&["daemon-reload"]);
        return Err(err.context("Could not persist Fatir agent schedule; timer was rolled back"));
    }

    Ok(json!({
        "scheduled": true,
        "job": item,
        "timer": timer_name,
        "persistent_across_reboot": true,
        "credential_scope": credential_ids,
        "credential_values_exposed": false
    }))
}

pub fn list() -> Result<Vec<Value>> {
    let mut items = load();
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(items.into_iter().map(|x| {
        let timer = format!("{}.timer", x.unit);
        let state = Command::new("systemctl")
            .args(["--user", "is-active", &timer])
            .output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".into());
        let next = Command::new("systemctl")
            .args(["--user", "show", &timer, "-p", "NextElapseUSecRealtime", "--value"])
            .output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        json!({
            "id":x.id,"label":x.label,"prompt":x.prompt,"trigger_kind":x.trigger_kind,"trigger":x.trigger,
            "calendar":x.calendar,"execution_mode":x.execution_mode,"credential_ids":x.credential_ids,
            "verification_required":x.verification_required,"session_id":x.session_id,"created_at":x.created_at,
            "last_run_at":x.last_run_at,"last_status":x.last_status,"last_result":x.last_result,
            "last_model":x.last_model,"last_pending_action":x.last_pending_action,
            "timer_state":state,"next":next
        })
    }).collect())
}

pub fn get(id: &str) -> Result<AgentSchedule> {
    load().into_iter().find(|x| x.id == id).ok_or_else(|| anyhow!("Agent schedule not found"))
}

pub fn cancel(id: &str) -> Result<Value> {
    let _guard = registry_lock().lock().map_err(|_| anyhow!("Schedule registry lock poisoned"))?;
    let mut items = load();
    let pos = items.iter().position(|x| x.id == id).ok_or_else(|| anyhow!("Agent schedule not found"))?;
    let item = items[pos].clone();
    let timer_name = format!("{}.timer", item.unit);
    let service_name = format!("{}.service", item.unit);
    let _ = Command::new("systemctl").args(["--user", "disable", "--now", &timer_name]).status();
    let _ = Command::new("systemctl").args(["--user", "stop", &service_name]).status();
    let _ = Command::new("systemctl").args(["--user", "reset-failed", &timer_name, &service_name]).status();
    let _ = fs::remove_file(unit_dir().join(&timer_name));
    let _ = fs::remove_file(unit_dir().join(&service_name));
    let _ = run_systemctl(&["daemon-reload"]);
    items.remove(pos);
    save(&items)?;
    Ok(json!({"cancelled":id,"timer":timer_name}))
}

pub fn run_now(id: &str) -> Result<Value> {
    let item = get(id)?;
    let exe = std::env::current_exe().context("Could not determine Fatir executable path")?;
    Command::new(exe)
        .args(["--background", "--run-agent-schedule", &item.id])
        .spawn()
        .context("Could not start Fatir scheduled agent run")?;
    Ok(json!({"queued":true,"id":item.id,"label":item.label}))
}

fn status_for_response(response: &AgentResponse) -> &'static str {
    if response.pending.is_some() {
        return "waiting_approval";
    }
    let lower = response.text.to_ascii_lowercase();
    if lower.contains("browser control is paused")
        || lower.contains("manual step")
        || lower.contains("authenticator")
        || lower.contains("verification code")
        || lower.contains("security key")
        || lower.contains("captcha")
        || lower.contains("check your phone")
        || lower.contains("approve sign-in")
    {
        "waiting_user"
    } else {
        "completed"
    }
}

fn update_after_run(id: &str, status: &str, response: Option<&AgentResponse>, error: Option<&str>) -> Result<()> {
    let _guard = registry_lock().lock().map_err(|_| anyhow!("Schedule registry lock poisoned"))?;
    let mut items = load();
    let item = items.iter_mut().find(|x| x.id == id).ok_or_else(|| anyhow!("Agent schedule not found"))?;
    item.last_run_at = Some(Utc::now().to_rfc3339());
    item.last_status = Some(status.into());
    item.last_pending_action = response.and_then(|r| r.pending.as_ref().map(|p| p.id.clone()));
    item.last_model = response.map(|r| r.model.clone());
    item.last_result = response
        .map(|r| r.text.chars().take(6000).collect())
        .or_else(|| error.map(|e| e.chars().take(6000).collect()));
    save(&items)?;

    fs::create_dir_all(data_dir())?;
    let row = json!({
        "schedule_id":id,
        "at":Utc::now().to_rfc3339(),
        "status":status,
        "model":response.map(|r|r.model.clone()),
        "pending_action":response.and_then(|r|r.pending.as_ref().map(|p|p.id.clone())),
        "result":response.map(|r|r.text.chars().take(6000).collect::<String>()),
        "error":error.map(|e|e.chars().take(6000).collect::<String>())
    });
    use std::io::Write;
    let mut file = fs::OpenOptions::new().create(true).append(true).open(run_log_path())?;
    writeln!(file, "{}", serde_json::to_string(&row)?)?;
    Ok(())
}

fn credential_context(item: &AgentSchedule) -> String {
    if item.credential_ids.is_empty() { return "No stored credentials were authorized for this schedule.".into(); }
    let allowed: HashSet<&str> = item.credential_ids.iter().map(String::as_str).collect();
    let rows = credentials::list().unwrap_or_default().into_iter().filter_map(|v| {
        let id = v.get("id").and_then(Value::as_str)?;
        if !allowed.contains(id) { return None; }
        let label = v.get("label").and_then(Value::as_str).unwrap_or("");
        let account = v.get("account").and_then(Value::as_str).unwrap_or("");
        Some(format!("- id={id} label={label:?} account={account:?}"))
    }).collect::<Vec<_>>();
    format!("Authorized stored credentials for this schedule (metadata only; secrets stay in the broker):\n{}", rows.join("\n"))
}

fn execution_instruction(item: &AgentSchedule) -> &'static str {
    match item.execution_mode.as_str() {
        "managed_browser" => "Use Fatir's visible managed browser for web work. Do not switch to the user's active Chrome and do not use headless mode.",
        "active_browser" => "Use the current active browser session exactly. If it is unavailable, stop and report the blocker; do not launch a replacement browser.",
        "headless_browser" => "The user explicitly authorized HEADLESS browser execution when creating this schedule. Run the web portion headless and keep it isolated from the visible browser.",
        _ => "Use normal Fatir agent tools. Headless browser execution is not authorized unless this schedule was explicitly created in headless_browser mode.",
    }
}

fn build_prompt(item: &AgentSchedule) -> String {
    format!(
        "FATIR SCHEDULED AGENT RUN\nSchedule: {}\nRun time: {}\nExecution authorization: {}\nVerification required: {}\n\n{}\n\nTask:\n{}\n\nRules for this scheduled run:\n- This task was explicitly scheduled by the user. Execute it rather than asking whether to begin.\n- Stored credential use is authorized ONLY for the credential IDs listed above. Use the credential broker; never expose secret values in text, logs, screenshots, model context, or chat.\n- If a page offers Continue/Sign in with Google and that matches the requested account flow, use the existing authenticated Google/browser session.\n- If an authenticator/TOTP code, security key, CAPTCHA, phone approval, unusual login verification, or account-recovery step is required, use browser_takeover with a precise reason and STOP. Never guess or request the secret in ordinary chat.\n- Do not claim completion until you verify the requested outcome. For publishing, verify the public/draft state and resulting URL or equivalent evidence.\n- If blocked, report exactly what is needed so Desktop and Companion can surface it.",
        item.label,
        Local::now().to_rfc3339(),
        execution_instruction(item),
        if item.verification_required { "yes" } else { "best-effort" },
        credential_context(item),
        item.prompt
    )
}

pub fn tool_preapproved(tool: &str, args: &Value) -> bool {
    SCHEDULE_AUTH.try_with(|auth| {
        match tool {
            "browser_fill_credential" | "browser_fill_credential_by_label" | "desktop_fill_credential" => {
                args.get("credential_id").and_then(Value::as_str)
                    .map(|id| auth.credential_ids.contains(id))
                    .unwrap_or(false)
            }
            "run_shell_with_credentials" => {
                let Some(map) = args.get("credentials").and_then(Value::as_object) else { return false; };
                !map.is_empty() && map.values().all(|v| {
                    v.as_str().map(|id| auth.credential_ids.contains(id)).unwrap_or(false)
                })
            }
            _ => false,
        }
    }).unwrap_or(false)
}

pub fn current_schedule_id() -> Option<String> {
    SCHEDULE_AUTH.try_with(|auth| auth.schedule_id.clone()).ok()
}

pub async fn approve_pending(state: SharedState, id: &str) -> Result<AgentResponse> {
    let item = get(id)?;
    let action_id = item
        .last_pending_action
        .clone()
        .ok_or_else(|| anyhow!("This scheduled agent is not waiting for an approval"))?;
    let auth = ScheduleAuthorization {
        schedule_id: item.id.clone(),
        credential_ids: item.credential_ids.iter().cloned().collect(),
    };
    let response = SCHEDULE_AUTH
        .scope(auth, ollama::approve_action(state, &action_id, "auto"))
        .await?;
    let status = status_for_response(&response);
    update_after_run(id, status, Some(&response), None)?;
    Ok(response)
}

pub async fn deny_pending(state: SharedState, id: &str) -> Result<AgentResponse> {
    let item = get(id)?;
    let action_id = item
        .last_pending_action
        .clone()
        .ok_or_else(|| anyhow!("This scheduled agent is not waiting for an approval"))?;
    let auth = ScheduleAuthorization {
        schedule_id: item.id.clone(),
        credential_ids: item.credential_ids.iter().cloned().collect(),
    };
    let response = SCHEDULE_AUTH
        .scope(auth, ollama::deny_action(state, &action_id, "auto"))
        .await?;
    update_after_run(id, "denied", Some(&response), None)?;
    Ok(response)
}

pub async fn execute(state: SharedState, id: &str) -> Result<Value> {
    let item = get(id)?;
    update_after_run(id, "running", None, None)?;
    let auth = ScheduleAuthorization {
        schedule_id: item.id.clone(),
        credential_ids: item.credential_ids.iter().cloned().collect(),
    };
    let prompt = build_prompt(&item);
    let _ = chats::touch(&item.session_id, &format!("Scheduled: {}", item.label));
    let response = SCHEDULE_AUTH
        .scope(auth, ollama::send_message(state.clone(), &item.session_id, &prompt, Vec::new(), "auto"))
        .await;

    match response {
        Ok(response) => {
            let status = status_for_response(&response);
            update_after_run(id, status, Some(&response), None)?;
            let summary = response.text.chars().take(240).collect::<String>();
            let _ = Command::new("notify-send")
                .arg(format!("Fatir · {}", item.label))
                .arg(if status == "completed" { summary.clone() } else { format!("{status}: {summary}") })
                .spawn();
            Ok(json!({
                "id":id,
                "status":status,
                "session_id":item.session_id,
                "response":response
            }))
        }
        Err(err) => {
            let text = err.to_string();
            update_after_run(id, "failed", None, Some(&text))?;
            let _ = Command::new("notify-send")
                .arg(format!("Fatir · {}", item.label))
                .arg(format!("Scheduled task failed: {}", text.chars().take(220).collect::<String>()))
                .spawn();
            Err(err)
        }
    }
}
