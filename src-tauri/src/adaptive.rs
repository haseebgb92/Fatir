use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveConfig {
    pub enabled: bool,
    pub local_first: bool,
    pub learn_routes: bool,
    pub suggest_routines: bool,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self { enabled: true, local_first: true, learn_routes: true, suggest_routines: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunRecord {
    timestamp: DateTime<Utc>,
    intent: String,
    route: String,
    model: String,
    success: bool,
    duration_ms: u64,
    tool_actions: usize,
    prompt_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    tools: Vec<String>,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
        .join("adaptive")
}

fn config_path() -> PathBuf { data_dir().join("config.json") }
fn runs_path() -> PathBuf { data_dir().join("runs.jsonl") }

pub fn config() -> AdaptiveConfig {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str::<AdaptiveConfig>(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(cfg: &AdaptiveConfig) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    fs::write(config_path(), serde_json::to_vec_pretty(cfg)?)?;
    Ok(())
}

pub fn clear() -> Result<()> {
    let path = runs_path();
    if path.exists() { fs::remove_file(path)?; }
    Ok(())
}

fn looks_like_web_address(h: &str) -> bool {
    if h.contains("http://") || h.contains("https://") || h.contains("www.") { return true; }
    h.split_whitespace().any(|raw| {
        let t=raw.trim_matches(|c:char| matches!(c,','|'.'|'!'|'?'|'\''|'"'|'('|')'|'['|']'|'{'|'}'|':'|';'));
        if t.starts_with('/') || t.starts_with("~/") || t.contains('/') { return false; }
        [".com",".org",".net",".io",".ai",".co",".dev",".app",".shop",".store"].iter().any(|suffix|t.to_ascii_lowercase().ends_with(suffix))
    })
}

pub fn explicit_headless(text:&str)->bool {
    let h=text.to_lowercase();
    h.split(|c:char|!c.is_alphanumeric() && c!='-').any(|t|t=="headless")
}


pub fn is_active_browser_request(text:&str)->bool {
    let h=text.to_lowercase();
    [
        "active browser session", "current browser session", "active chrome session", "current chrome session",
        "same browser session", "same chrome session", "browser i am using", "chrome i am using",
        "my current browser", "my current chrome", "existing browser session", "existing chrome session",
        "active chrome", "current chrome"
    ].iter().any(|needle| h.contains(needle))
}

pub fn has_explicit_local_surface(text:&str)->bool {
    let h=text.to_lowercase();
    let local=[
        "linux mint", "desktop app", "desktop application", "installed app", "installed application", "local app",
        "nemo", "file manager", "terminal", "shell", "command line", "run command", "local file", "local folder",
        "android studio", "at-spi", "pyatspi", "accessibility", "systemctl", "apt ", "apt-get ", "flatpak "
    ];
    h.contains("~/") || h.contains("/home/") || h.contains("/usr/") || h.contains("/etc/") || h.contains("/opt/")
        || local.iter().any(|needle|h.contains(needle))
}

pub fn is_browser_request(text:&str)->bool {
    let h=text.to_lowercase();
    if explicit_headless(text){return true;}
    if is_active_browser_request(text){return true;}

    // Strong local/CLI context must not launch Chrome merely because a command
    // contains a URL (curl/wget/npm/etc.) or generic UI words such as click/tab.
    let local_cli=[
        "terminal", "shell", "command line", "run command", "run this command", "sudo ", "apt ", "apt-get ",
        "flatpak ", "systemctl", "journalctl", "cargo ", "rustc ", "npm ", "npx ", "pnpm ", "yarn ", "pip ",
        "python ", "python3 ", "bash ", "zsh ", "chmod ", "chown ", "curl ", "wget ", "unzip ", "tar ", "git clone",
        "linux mint", "nemo", "cinnamon", "file manager", "local file", "local folder", "local path",
        "android studio", "desktop control", "at-spi", "pyatspi", "accessibility", "work locally", "local application", "local app"
    ];
    let local_path = h.contains("~/") || h.contains("/home/") || h.contains("/usr/") || h.contains("/etc/") || h.contains("/opt/") || h.contains("./");
    let strong_local = local_path || local_cli.iter().any(|x|h.contains(x));
    let explicit_browser_surface=[
        "browser","website","webpage","web page","in chrome","chrome browser","open chrome","launch chrome","start chrome","use chrome",
        "current chrome","active chrome","chrome session","current browser","active browser","browser session",
        "chrome tab","chrome tabs","browser tab","browser tabs","open the site","visit the site","browse online","search online"
    ];
    let asks_browser_surface=explicit_browser_surface.iter().any(|x|h.contains(x));
    if strong_local && !asks_browser_surface {
        return false;
    }

    // Installing/removing a web app is a software task unless the request also
    // explicitly asks to use its web surface.
    let software_action=["install ","uninstall ","remove package","update package","upgrade package"];
    if software_action.iter().any(|x|h.contains(x)) && !asks_browser_surface && !looks_like_web_address(&h) {
        return false;
    }

    if asks_browser_surface { return true; }
    if looks_like_web_address(&h){return true;}
    let explicit=[
        "browser", "website", "webpage", "web page", "search online", "browse online", "visit the site", "open the site",
        "whatsapp", "gmail", "amazon", "ebay", "etsy", "shopify", "fiverr", "facebook", "instagram", "linkedin",
        "youtube", "reddit", "google search", "google.com", "seller central", "wordpress admin", "wp-admin"
    ];
    explicit.iter().any(|needle|h.contains(needle))
}

fn normalize_intent(text: &str) -> String {
    let h = text.to_lowercase();
    let contains_any = |words: &[&str]| words.iter().any(|w| h.contains(w));
    // Explicit web context is required for browser routing. Generic UI words such as
    // click/tab/page/login/upload are intentionally NOT browser signals because they
    // also occur in local Linux and desktop-app tasks.
    if is_browser_request(text) { return "browser".into(); }
    if contains_any(&["install", "uninstall", "apt ", "flatpak", "package", "software", "appimage", ".deb", "update "]) { return "software".into(); }
    let trimmed=h.trim_start();
    let launch_like=["open ","launch ","start ","use ","control "].iter().any(|p|trimmed.starts_with(p));
    let path_like=h.contains("~/") || h.contains("/home/") || h.contains("./") || contains_any(&["file","folder","directory","path"]);
    if launch_like && !path_like { return "desktop".into(); }
    if contains_any(&["desktop", "desktop app", "desktop application", "open applications", "open apps", "installed app", "installed apps", "installed application", "installed applications", "operate installed", "control installed", "window", "windows", "at-spi", "accessibility control", "computer control", "dialog", "local app"]) { return "desktop".into(); }
    if contains_any(&["schedule", "scheduled", "scheduler", "remind me", "reminder", "timer", "every day", "every week", "daily", "weekly", "monthly"]) { return "system".into(); }
    if contains_any(&["trash", "downloads", "duplicate", "clean", "storage", "disk space", "largest file", "largest folder", "free space"]) { return "cleanup".into(); }
    if contains_any(&["file", "folder", "directory", "pdf", "document", "move ", "copy ", "rename ", "extract ", "archive", "local path", "home folder"]) { return "files".into(); }
    if contains_any(&["service", "systemctl", "process", "cpu", "ram", "memory", "linux", "mint", "terminal", "command", "shell", "permission", "mount", "network", "port", "system ", "nemo", "cinnamon"]) { return "system".into(); }
    if contains_any(&["code", "compile", "build", "cargo", "rust", "python", "javascript", "typescript", "android", "gradle", "debug", "error", "stack trace", "repository", "git "]) { return "coding".into(); }
    if contains_any(&["research", "compare", "latest", "news", "find information", "investigate"]) { return "research".into(); }
    "general".into()
}

pub fn intent_for(text: &str) -> String { normalize_intent(text) }

fn route_for_model(model: &str) -> String {
    let m = model.to_lowercase();
    if m.contains("fast path") { "fast-path".into() }
    else if m.starts_with("fatir-router") || m.starts_with("fatir-local") || (m.contains("qwen3.5") && !m.contains(":cloud")) { "local".into() }
    else if m.contains(":cloud") || m.contains("gpt-oss") || m.contains("qwen3-vl") || m.contains("gemma4") { "cloud".into() }
    else { "other".into() }
}

fn parse_efficiency(trace: &[crate::models::TraceItem]) -> (usize, u64, u64) {
    let mut tool_actions = 0usize;
    let mut prompt_tokens = 0u64;
    let mut output_tokens = 0u64;
    for item in trace {
        if item.status != "meta" { tool_actions += 1; }
        if item.status == "meta" && item.detail.contains("prompt +") {
            let parts: Vec<&str> = item.detail.split_whitespace().collect();
            for i in 0..parts.len() {
                if parts.get(i + 1) == Some(&"prompt") {
                    prompt_tokens = parts[i].replace(',', "").parse().unwrap_or(0);
                }
                if parts.get(i + 1) == Some(&"output") {
                    output_tokens = parts[i].replace(',', "").parse().unwrap_or(0);
                }
            }
        }
    }
    (tool_actions, prompt_tokens, output_tokens)
}


pub fn note_user_correction(text: &str) -> Result<()> {
    let h = text.trim().to_lowercase();
    let correction = [
        "that didn't work", "that did not work", "it didn't work", "it did not work",
        "that's wrong", "that is wrong", "wrong result", "still not working",
        "couldn't", "could not", "failed again", "not what i asked"
    ].iter().any(|p| h.contains(p));
    if !correction { return Ok(()); }
    let mut rows = read_records(600);
    if let Some(last) = rows.last_mut() { last.success = false; }
    fs::create_dir_all(data_dir())?;
    let mut out = String::new();
    for r in rows { out.push_str(&serde_json::to_string(&r)?); out.push('\n'); }
    fs::write(runs_path(), out)?;
    Ok(())
}

pub fn record_interaction(user_text: &str, response: &crate::models::AgentResponse, duration_ms: u64) -> Result<()> {
    let cfg = config();
    if !cfg.enabled || response.pending.is_some() { return Ok(()); }
    fs::create_dir_all(data_dir())?;
    let (tool_actions, prompt_tokens, output_tokens) = parse_efficiency(&response.trace);
    let success = !response.trace.iter().any(|t| t.status == "error")
        && !response.text.to_lowercase().contains("i paused this run")
        && !response.text.to_lowercase().contains("i hit a problem");
    let tools = response.trace.iter()
        .filter(|t| t.status != "meta")
        .map(|t| t.title.clone())
        .filter(|t| !t.trim().is_empty())
        .take(24)
        .collect();
    let record = RunRecord {
        timestamp: Utc::now(),
        intent: normalize_intent(user_text),
        route: route_for_model(&response.model),
        model: response.model.clone(),
        success,
        duration_ms,
        tool_actions,
        prompt_tokens,
        output_tokens,
        tools,
    };
    use std::io::Write;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(runs_path())?;
    writeln!(f, "{}", serde_json::to_string(&record)?)?;
    trim_records(600)?;
    Ok(())
}

fn read_records(limit: usize) -> Vec<RunRecord> {
    let Ok(text) = fs::read_to_string(runs_path()) else { return Vec::new(); };
    let mut rows: Vec<RunRecord> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    if rows.len() > limit { rows.drain(0..rows.len() - limit); }
    rows
}

fn trim_records(max: usize) -> Result<()> {
    let rows = read_records(max + 200);
    if rows.len() <= max { return Ok(()); }
    let keep = &rows[rows.len() - max..];
    let mut out = String::new();
    for r in keep { out.push_str(&serde_json::to_string(r)?); out.push('\n'); }
    fs::write(runs_path(), out)?;
    Ok(())
}

#[derive(Default)]
struct Stat {
    runs: usize,
    successes: usize,
    duration_ms: u128,
    prompt_tokens: u128,
    output_tokens: u128,
}

fn aggregate_for(intent: &str) -> HashMap<String, Stat> {
    let mut map: HashMap<String, Stat> = HashMap::new();
    for r in read_records(400).into_iter().filter(|r| r.intent == intent) {
        let s = map.entry(r.route).or_default();
        s.runs += 1;
        if r.success { s.successes += 1; }
        s.duration_ms += r.duration_ms as u128;
        s.prompt_tokens += r.prompt_tokens as u128;
        s.output_tokens += r.output_tokens as u128;
    }
    map
}

pub fn preferred_route(text: &str) -> Option<String> {
    let cfg = config();
    if !cfg.enabled || !cfg.learn_routes { return None; }
    let intent = normalize_intent(text);
    let stats = aggregate_for(&intent);
    let local = stats.get("local");
    let cloud = stats.get("cloud");
    if let Some(s) = local {
        if s.runs >= 3 && s.successes * 100 / s.runs >= 75 { return Some("local".into()); }
        if s.runs >= 4 && s.successes * 100 / s.runs < 50 {
            if let Some(c) = cloud {
                if c.runs >= 2 && c.successes * 100 / c.runs >= 75 { return Some("cloud".into()); }
            }
        }
    }
    None
}

pub fn context_for(text: &str) -> String {
    let cfg = config();
    if !cfg.enabled { return String::new(); }
    let intent = normalize_intent(text);
    let stats = aggregate_for(&intent);
    if stats.is_empty() { return String::new(); }
    let mut parts = Vec::new();
    for route in ["fast-path", "local", "cloud"] {
        if let Some(s) = stats.get(route) {
            let avg_ms = if s.runs == 0 { 0 } else { (s.duration_ms / s.runs as u128) as u64 };
            parts.push(format!("{route}: {}/{} successful, avg {} ms", s.successes, s.runs, avg_ms));
        }
    }
    let rows = read_records(250);
    let mut seq_counts: HashMap<String, usize> = HashMap::new();
    for r in rows.into_iter().filter(|r| r.intent == intent && r.success && !r.tools.is_empty()) {
        *seq_counts.entry(r.tools.join(" → ")).or_default() += 1;
    }
    let proven = seq_counts.into_iter().max_by_key(|(_, count)| *count)
        .filter(|(_, count)| *count >= 2)
        .map(|(seq, count)| format!(" Proven recent tool sequence ({count} successes): {seq}."))
        .unwrap_or_default();
    if parts.is_empty() && proven.is_empty() { String::new() }
    else { format!("ADAPTIVE ROUTING HISTORY for intent '{intent}' (observational only; never bypass approvals): {}{}", parts.join("; "), proven) }
}

pub fn suggestions(limit: usize) -> Vec<Value> {
    let cfg = config();
    if !cfg.enabled { return Vec::new(); }
    let rows = read_records(400);
    let mut by_intent: HashMap<String, HashMap<String, Stat>> = HashMap::new();
    for r in rows {
        let s = by_intent.entry(r.intent).or_default().entry(r.route).or_default();
        s.runs += 1;
        if r.success { s.successes += 1; }
        s.duration_ms += r.duration_ms as u128;
        s.prompt_tokens += r.prompt_tokens as u128;
        s.output_tokens += r.output_tokens as u128;
    }
    let mut out = Vec::new();
    for (intent, routes) in by_intent {
        if let Some(local) = routes.get("local") {
            let pct = if local.runs == 0 { 0 } else { local.successes * 100 / local.runs };
            if local.runs >= 3 && pct >= 80 {
                out.push(json!({
                    "kind":"routing",
                    "title":format!("Keep {} work local", intent),
                    "detail":format!("Local handling succeeded on {}/{} recent {} runs. Fatir can continue preferring the local model for this class.", local.successes, local.runs, intent),
                    "confidence":pct
                }));
            }
        }
        let total: usize = routes.values().map(|s| s.runs).sum();
        if cfg.suggest_routines && total >= 4 {
            out.push(json!({
                "kind":"routine",
                "title":format!("Review repeated {} workflow", intent),
                "detail":format!("Fatir has seen {} recent runs in this class. If they represent the same workflow, capture the next successful sequence as a reusable routine.", total),
                "confidence":70
            }));
        }
    }
    if cfg.suggest_routines {
        let mut sequences: HashMap<String, (String, Vec<String>, usize)> = HashMap::new();
        for r in read_records(300).into_iter().filter(|r| r.success && r.tools.len() >= 2) {
            let signature = format!("{}::{}", r.intent, r.tools.join(">"));
            let entry = sequences.entry(signature).or_insert((r.intent.clone(), r.tools.clone(), 0));
            entry.2 += 1;
        }
        for (_sig, (intent, tools, count)) in sequences.into_iter().filter(|(_, (_, _, count))| *count >= 3) {
            out.push(json!({
                "kind":"routine-sequence",
                "title":format!("Turn a proven {} sequence into a routine", intent),
                "detail":format!("This sequence succeeded {} times: {}. Fatir will only save/run it through the normal routine and approval system.", count, tools.join(" → ")),
                "confidence":90
            }));
        }
    }
    out.sort_by_key(|v| std::cmp::Reverse(v.get("confidence").and_then(Value::as_u64).unwrap_or(0)));
    out.truncate(limit);
    out
}

pub fn status() -> Value {
    let cfg = config();
    let rows = read_records(400);
    let total = rows.len();
    let successful = rows.iter().filter(|r| r.success).count();
    let local = rows.iter().filter(|r| r.route == "local").count();
    let cloud = rows.iter().filter(|r| r.route == "cloud").count();
    let fast = rows.iter().filter(|r| r.route == "fast-path").count();
    json!({
        "config": cfg,
        "stats": {
            "runs": total,
            "successful": successful,
            "local": local,
            "cloud": cloud,
            "fast_path": fast,
            "success_rate": if total == 0 { 0 } else { successful * 100 / total }
        },
        "suggestions": suggestions(6)
    })
}

pub fn ensure_layout() -> Result<()> {
    fs::create_dir_all(data_dir()).context("Could not create Fatir adaptive-learning directory")?;
    if !config_path().exists() { save_config(&AdaptiveConfig::default())?; }
    Ok(())
}

#[cfg(test)]
mod browser_routing_tests {
    use super::{explicit_headless,is_active_browser_request,is_browser_request,intent_for};

    #[test]
    fn generic_local_ui_words_do_not_launch_browser() {
        assert!(!is_browser_request("Click the new tab in Nemo and rename the local file"));
        assert!(!is_browser_request("Open Linux Mint settings and click the display button"));
        assert!(!is_browser_request("Get access to desktop tools so you can work locally instead of opening Chrome"));
        assert!(!is_browser_request("Can you control Android Studio?"));
        assert!(!is_browser_request("Can you operate installed apps?"));
        assert!(!is_browser_request("Check your local desktop capabilities."));
    }

    #[test]
    fn cli_urls_stay_on_the_local_surface() {
        assert!(!is_browser_request("Run curl https://example.com/api in the terminal"));
        assert!(!is_browser_request("Use wget https://example.com/file.zip from Linux Mint"));
    }

    #[test]
    fn installed_app_launches_stay_desktop() {
        assert_eq!(intent_for("Open GIMP"), "desktop");
        assert_eq!(intent_for("Launch VLC"), "desktop");
        assert_eq!(intent_for("Control Calculator"), "desktop");
        assert_eq!(intent_for("Can you operate installed apps?"), "desktop");
        assert_eq!(intent_for("Can you control installed applications?"), "desktop");
        assert!(!is_browser_request("Open GIMP and crop this local image"));
        assert!(!is_browser_request("Remind me every day to run the backup"));
        assert_eq!(intent_for("Remind me every day to run the backup"), "system");
    }

    #[test]
    fn explicit_web_requests_use_browser() {
        assert!(is_browser_request("Open Amazon and search for dog leash"));
        assert!(is_browser_request("Open Chrome"));
        assert!(is_browser_request("Visit https://example.com in the browser"));
        assert!(is_browser_request("Send this file on WhatsApp"));
    }


    #[test]
    fn active_browser_phrases_mean_existing_user_chrome() {
        for text in [
            "In my current Chrome session, go to the Ollama tab",
            "Use the active browser session",
            "Check the current browser session",
            "Operate the Chrome I am using"
        ] {
            assert!(is_browser_request(text), "expected browser routing for: {text}");
            assert!(is_active_browser_request(text), "expected active-session routing for: {text}");
        }
    }

    #[test]
    fn headless_requires_the_literal_headless_request() {
        assert!(explicit_headless("Run this headless"));
        assert!(!explicit_headless("Run this in the background"));
        assert!(is_browser_request("Do this headless"));
    }
}
