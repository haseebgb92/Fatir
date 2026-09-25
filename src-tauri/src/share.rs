use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, io::Write, path::{Path, PathBuf}, process::{Command, Stdio}};

use crate::tools;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareFile {
    pub path: String,
    pub name: String,
    pub mime: String,
    pub bytes: u64,
    pub modified: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShareHistoryEntry {
    timestamp: DateTime<Utc>,
    destination: String,
    recipient: String,
    paths: Vec<String>,
    status: String,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
        .join("share")
}
fn history_path() -> PathBuf { data_dir().join("history.jsonl") }
fn contacts_path() -> PathBuf { data_dir().join("whatsapp-contacts.json") }

fn expand(input: &str) -> PathBuf {
    if input == "~" { return dirs::home_dir().unwrap_or_default(); }
    if let Some(rest) = input.strip_prefix("~/") { return dirs::home_dir().unwrap_or_default().join(rest); }
    PathBuf::from(input)
}

fn validated_paths(paths: &[String]) -> Result<Vec<PathBuf>> {
    if paths.is_empty() { return Err(anyhow!("Choose at least one local file to share")); }
    if paths.len() > 25 { return Err(anyhow!("Fatir Share supports up to 25 files at a time")); }
    let mut out = Vec::with_capacity(paths.len());
    for raw in paths {
        let p = expand(raw).canonicalize().with_context(|| format!("Cannot access {}", raw))?;
        if !p.is_file() { return Err(anyhow!("Only regular files can be shared: {}", p.display())); }
        out.push(p);
    }
    Ok(out)
}

pub fn describe(paths: &[String]) -> Result<Vec<ShareFile>> {
    let paths = validated_paths(paths)?;
    paths.into_iter().map(|p| {
        let meta = fs::metadata(&p)?;
        let modified = meta.modified().ok().map(DateTime::<Utc>::from);
        Ok(ShareFile {
            path: p.display().to_string(),
            name: p.file_name().and_then(|x| x.to_str()).unwrap_or("file").to_string(),
            mime: mime_guess::from_path(&p).first_or_octet_stream().essence_str().to_string(),
            bytes: meta.len(),
            modified,
        })
    }).collect()
}

fn run_xclip(target: &str, bytes: &[u8]) -> Result<()> {
    let mut child = Command::new("xclip")
        .args(["-selection", "clipboard", "-t", target, "-i"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn().context("xclip is required for Fatir Share clipboard support")?;
    if let Some(stdin) = child.stdin.as_mut() { stdin.write_all(bytes)?; }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(anyhow!("Clipboard copy failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

pub fn copy(paths: &[String]) -> Result<Value> {
    let files = describe(paths)?;
    if files.len() == 1 && files[0].mime.starts_with("image/") {
        let bytes = fs::read(&files[0].path)?;
        run_xclip(&files[0].mime, &bytes)?;
        return Ok(json!({"copied":"image","count":1,"path":files[0].path,"mime":files[0].mime}));
    }
    let mut uris = String::new();
    for file in &files {
        let uri = url::Url::from_file_path(Path::new(&file.path)).map_err(|_| anyhow!("Could not convert {} to file URI", file.path))?;
        uris.push_str(uri.as_str());
        uris.push_str("\r\n");
    }
    run_xclip("text/uri-list", uris.as_bytes())?;
    Ok(json!({"copied":"files","count":files.len(),"paths":files.iter().map(|f|f.path.clone()).collect::<Vec<_>>()}))
}

pub fn copy_paths(paths: &[String]) -> Result<Value> {
    let files = describe(paths)?;
    let text = files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>().join("\n");
    run_xclip("UTF8_STRING", text.as_bytes())?;
    Ok(json!({"copied":"paths","count":files.len()}))
}

pub fn latest_screenshot() -> Result<ShareFile> {
    let picture = dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"));
    let candidates = [picture.join("screenshot"), picture.join("Fatir")];
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for dir in candidates {
        let Ok(entries) = fs::read_dir(dir) else { continue; };
        for entry in entries.flatten() {
            let p = entry.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if !["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) { continue; }
            let Ok(meta) = entry.metadata() else { continue; };
            let Ok(modified) = meta.modified() else { continue; };
            if newest.as_ref().map(|(t,_)| modified > *t).unwrap_or(true) { newest = Some((modified, p)); }
        }
    }
    let path = newest.map(|(_,p)| p).ok_or_else(|| anyhow!("No screenshot found in ~/Pictures/screenshot or ~/Pictures/Fatir"))?;
    describe(&[path.display().to_string()])?.into_iter().next().ok_or_else(|| anyhow!("Screenshot disappeared"))
}

fn append_history(destination: &str, recipient: &str, paths: &[PathBuf], status: &str) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let item = ShareHistoryEntry {
        timestamp: Utc::now(), destination: destination.into(), recipient: recipient.into(),
        paths: paths.iter().map(|p|p.display().to_string()).collect(), status: status.into(),
    };
    let mut f = fs::OpenOptions::new().create(true).append(true).open(history_path())?;
    writeln!(f, "{}", serde_json::to_string(&item)?)?;
    Ok(())
}

pub fn history(limit: usize) -> Vec<Value> {
    let Ok(text) = fs::read_to_string(history_path()) else { return Vec::new(); };
    let mut rows: Vec<ShareHistoryEntry> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    if rows.len() > limit { rows.drain(0..rows.len()-limit); }
    rows.into_iter().rev().map(|x| serde_json::to_value(x).unwrap_or(Value::Null)).collect()
}

fn read_cached_contacts() -> Vec<String> {
    fs::read_to_string(contacts_path()).ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}
fn save_cached_contacts(items: &[String]) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    fs::write(contacts_path(), serde_json::to_vec_pretty(items)?)?;
    Ok(())
}

pub async fn whatsapp_contacts() -> Result<Value> {
    match tools::share_whatsapp_contacts_live().await {
        Ok(mut items) => {
            items.sort_by_key(|x| x.to_lowercase()); items.dedup_by(|a,b| a.eq_ignore_ascii_case(b));
            if !items.is_empty() { let _ = save_cached_contacts(&items); }
            Ok(json!({"contacts":items,"source":"live"}))
        }
        Err(err) => {
            let cached = read_cached_contacts();
            if cached.is_empty() { Err(err) }
            else { Ok(json!({"contacts":cached,"source":"cache","warning":err.to_string()})) }
        }
    }
}

pub async fn whatsapp_send(paths: &[String], contact: &str) -> Result<Value> {
    let contact = contact.trim();
    if contact.is_empty() { return Err(anyhow!("Choose a WhatsApp recipient")); }
    let files = validated_paths(paths)?;
    let result = tools::share_whatsapp_send_files(&files, contact).await?;
    let _ = append_history("whatsapp", contact, &files, "sent");
    Ok(result)
}

pub async fn email_draft(paths: &[String], to: &str, subject: &str) -> Result<Value> {
    let to = to.trim();
    if to.is_empty() || !to.contains('@') { return Err(anyhow!("Enter a valid email address")); }
    let files = validated_paths(paths)?;
    let result = tools::share_email_draft_files(&files, to, subject.trim()).await?;
    let _ = append_history("email", to, &files, "drafted");
    Ok(result)
}
