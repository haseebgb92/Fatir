use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, path::{Path, PathBuf}, process::Command, time::SystemTime};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupCandidate {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub human: String,
    pub age_days: u64,
    pub category: String,
    pub confidence: String,
    pub reason: String,
    pub recommended_action: String,
}

fn home() -> PathBuf { dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")) }
fn downloads() -> PathBuf { home().join("Downloads") }
fn trash_files() -> PathBuf { home().join(".local/share/Trash/files") }

fn expand(input: Option<&str>) -> PathBuf {
    let raw = input.unwrap_or("").trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("downloads") { return downloads(); }
    if raw.eq_ignore_ascii_case("trash") { return trash_files(); }
    if raw == "~" { return home(); }
    if let Some(rest) = raw.strip_prefix("~/") { return home().join(rest); }
    PathBuf::from(raw)
}

fn human(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut idx = 0usize;
    while value >= 1024.0 && idx < units.len() - 1 { value /= 1024.0; idx += 1; }
    if idx == 0 { format!("{} {}", bytes, units[idx]) } else { format!("{value:.1} {}", units[idx]) }
}

fn age_days(meta: &fs::Metadata) -> u64 {
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    SystemTime::now().duration_since(modified).map(|d| d.as_secs() / 86_400).unwrap_or(0)
}

fn ext(path: &Path) -> String {
    path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn extracted_peer_exists(path: &Path) -> bool {
    let Some(parent) = path.parent() else { return false; };
    let Some(file) = path.file_name().and_then(|x| x.to_str()) else { return false; };
    let lower = file.to_ascii_lowercase();
    let stems = [".tar.gz", ".tar.xz", ".tar.bz2", ".tgz", ".zip", ".7z", ".rar", ".tar"];
    for suffix in stems {
        if lower.ends_with(suffix) {
            let stem = &file[..file.len().saturating_sub(suffix.len())];
            if parent.join(stem).is_dir() { return true; }
        }
    }
    false
}

fn classify(path: &Path, meta: &fs::Metadata, min_age_days: u64) -> Option<CleanupCandidate> {
    if !meta.is_file() { return None; }
    let bytes = meta.len();
    let age = age_days(meta);
    let name = path.file_name().and_then(|x| x.to_str()).unwrap_or("file").to_string();
    let extension = ext(path);
    let lower = name.to_ascii_lowercase();

    let (category, confidence, reason, action) = if ["crdownload", "part", "partial", "download"].contains(&extension.as_str()) && age >= 1 {
        ("partial-download", "high", "Incomplete download left behind for at least a day", "trash")
    } else if ["deb", "rpm", "appimage", "iso", "dmg", "exe", "msi", "pkg"].contains(&extension.as_str()) && age >= min_age_days.max(7) {
        ("installer", "medium", "Old installer image/package; verify the application is already installed before removing it", "review")
    } else if ["zip", "7z", "rar", "tgz", "tar", "gz", "xz", "bz2"].contains(&extension.as_str()) && age >= min_age_days.max(14) && extracted_peer_exists(path) {
        ("archive", "high", "Archive has a matching extracted folder beside it", "trash")
    } else if (lower.contains(" copy") || lower.contains("(1)") || lower.contains("(2)")) && age >= min_age_days.max(14) {
        ("possible-duplicate", "low", "Filename looks like a downloaded duplicate; compare before removing", "review")
    } else if bytes >= 1024 * 1024 * 1024 && age >= min_age_days.max(60) {
        ("large-old-file", "low", "Large file has not been modified for a long time; size alone does not make it safe to remove", "review")
    } else {
        return None;
    };

    Some(CleanupCandidate {
        path: path.display().to_string(), name, bytes, human: human(bytes), age_days: age,
        category: category.into(), confidence: confidence.into(), reason: reason.into(), recommended_action: action.into(),
    })
}

pub fn scan(path: Option<&str>, min_age_days: u64, limit: usize) -> Result<Value> {
    let root = expand(path);
    if !root.exists() { return Err(anyhow!("Cleanup path does not exist: {}", root.display())); }
    if !root.is_dir() { return Err(anyhow!("Cleanup path is not a directory: {}", root.display())); }

    let mut candidates = Vec::new();
    let mut scanned_files = 0usize;
    let mut scanned_bytes = 0u64;
    let max_items = limit.clamp(1, 500);

    for entry in WalkDir::new(&root).follow_links(false).max_depth(6).into_iter().filter_map(|e| e.ok()).take(30_000) {
        if !entry.file_type().is_file() { continue; }
        let Ok(meta) = entry.metadata() else { continue; };
        scanned_files += 1;
        scanned_bytes = scanned_bytes.saturating_add(meta.len());
        if let Some(candidate) = classify(entry.path(), &meta, min_age_days) { candidates.push(candidate); }
    }
    candidates.sort_by(|a, b| {
        let rank = |x: &str| match x { "high" => 3, "medium" => 2, _ => 1 };
        rank(&b.confidence).cmp(&rank(&a.confidence)).then_with(|| b.bytes.cmp(&a.bytes))
    });
    candidates.truncate(max_items);

    let high_bytes: u64 = candidates.iter().filter(|x| x.confidence == "high" && x.recommended_action == "trash").map(|x| x.bytes).sum();
    let review_bytes: u64 = candidates.iter().filter(|x| x.recommended_action == "review").map(|x| x.bytes).sum();
    Ok(json!({
        "root": root.display().to_string(),
        "scanned_files": scanned_files,
        "scanned_bytes": scanned_bytes,
        "candidate_count": candidates.len(),
        "high_confidence_reclaimable_bytes": high_bytes,
        "high_confidence_reclaimable": human(high_bytes),
        "review_bytes": review_bytes,
        "review_size": human(review_bytes),
        "candidates": candidates,
        "note": "High-confidence items are still moved to Trash first. Personal documents, source code, photos and arbitrary old files are never classified as safe solely because of age or size."
    }))
}

pub fn trash_summary(limit: usize) -> Result<Value> {
    let root = trash_files();
    if !root.exists() { return Ok(json!({"root":root.display().to_string(),"count":0,"bytes":0,"human":"0 B","items":[]})); }
    let mut total = 0u64;
    let mut total_count = 0usize;
    let mut items = Vec::new();
    for entry in WalkDir::new(&root).follow_links(false).min_depth(1).into_iter().filter_map(|e| e.ok()).take(40_000) {
        if !entry.file_type().is_file() { continue; }
        let Ok(meta) = entry.metadata() else { continue; };
        total = total.saturating_add(meta.len());
        total_count += 1;
        if items.len() < limit.clamp(1, 200) {
            items.push(json!({"path":entry.path().display().to_string(),"name":entry.file_name().to_string_lossy(),"bytes":meta.len(),"human":human(meta.len()),"age_days":age_days(&meta)}));
        }
    }
    items.sort_by(|a,b| b.get("bytes").and_then(Value::as_u64).unwrap_or(0).cmp(&a.get("bytes").and_then(Value::as_u64).unwrap_or(0)));
    Ok(json!({"root":root.display().to_string(),"count":total_count,"bytes":total,"human":human(total),"items":items}))
}

pub fn duplicate_scan(path: Option<&str>, minimum_size_mb: u64, limit_groups: usize) -> Result<Value> {
    let root = expand(path);
    if !root.is_dir() { return Err(anyhow!("Duplicate scan path is not a directory: {}", root.display())); }
    let min = minimum_size_mb.max(1) * 1024 * 1024;
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for entry in WalkDir::new(&root).follow_links(false).max_depth(8).into_iter().filter_map(|e| e.ok()).take(50_000) {
        if !entry.file_type().is_file() { continue; }
        let Ok(meta) = entry.metadata() else { continue; };
        if meta.len() >= min { by_size.entry(meta.len()).or_default().push(entry.path().to_path_buf()); }
    }
    let mut groups = Vec::new();
    for (size, paths) in by_size.into_iter().filter(|(_,v)| v.len() > 1) {
        let mut by_hash: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for p in paths.into_iter().take(20) {
            let out = Command::new("sha256sum").arg(&p).output();
            let Ok(out) = out else { continue; };
            if !out.status.success() { continue; }
            let hash = String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or("").to_string();
            if !hash.is_empty() { by_hash.entry(hash).or_default().push(p); }
        }
        for (hash, files) in by_hash.into_iter().filter(|(_,v)| v.len() > 1) {
            let reclaimable = size.saturating_mul(files.len().saturating_sub(1) as u64);
            groups.push(json!({"sha256":hash,"bytes_each":size,"human_each":human(size),"copies":files.len(),"reclaimable_bytes":reclaimable,"reclaimable":human(reclaimable),"paths":files.iter().map(|p|p.display().to_string()).collect::<Vec<_>>() }));
        }
    }
    groups.sort_by(|a,b| b.get("reclaimable_bytes").and_then(Value::as_u64).unwrap_or(0).cmp(&a.get("reclaimable_bytes").and_then(Value::as_u64).unwrap_or(0)));
    groups.truncate(limit_groups.clamp(1, 100));
    let reclaimable: u64 = groups.iter().map(|g|g.get("reclaimable_bytes").and_then(Value::as_u64).unwrap_or(0)).sum();
    Ok(json!({"root":root.display().to_string(),"groups":groups,"reclaimable_bytes":reclaimable,"reclaimable":human(reclaimable),"note":"Duplicate groups are byte-for-byte identical by SHA-256. Fatir must still keep at least one copy and should prefer moving extras to Trash."}))
}

pub fn move_many_to_trash(paths: &[String]) -> Result<Value> {
    if paths.is_empty() { return Err(anyhow!("No paths supplied")); }
    if paths.len() > 100 { return Err(anyhow!("Refusing to move more than 100 items to Trash in one action")); }
    let mut moved = Vec::new();
    let mut failed = Vec::new();
    for raw in paths {
        let p = expand(Some(raw));
        if !p.exists() { failed.push(json!({"path":raw,"error":"not found"})); continue; }
        let status = Command::new("gio").arg("trash").arg(&p).status();
        match status {
            Ok(s) if s.success() => moved.push(p.display().to_string()),
            Ok(_) => failed.push(json!({"path":p.display().to_string(),"error":"gio trash failed"})),
            Err(e) => failed.push(json!({"path":p.display().to_string(),"error":e.to_string()})),
        }
    }
    Ok(json!({"moved":moved,"failed":failed,"note":"Items were moved to desktop Trash, not permanently erased."}))
}

pub fn empty_trash() -> Result<Value> {
    let before = trash_summary(1)?;
    let status = Command::new("gio").args(["trash", "--empty"]).status().context("Could not invoke gio trash --empty")?;
    if !status.success() { return Err(anyhow!("Could not empty desktop Trash")); }
    Ok(json!({"ok":true,"previous_bytes":before.get("bytes").and_then(Value::as_u64).unwrap_or(0),"previous_size":before.get("human").and_then(Value::as_str).unwrap_or("0 B"),"emptied_at":DateTime::<Utc>::from(SystemTime::now()).to_rfc3339()}))
}
