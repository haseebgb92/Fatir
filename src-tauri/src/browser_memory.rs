use anyhow::Result;
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SemanticTarget {
    pub domain: String,
    pub action: String,
    pub role: String,
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub successes: u64,
    #[serde(default)]
    pub failures: u64,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UidTarget {
    pub uid: String,
    pub page_id: u32,
    pub domain: String,
    pub role: String,
    pub label: String,
    pub seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Store {
    #[serde(default)]
    targets: Vec<SemanticTarget>,
    #[serde(default)]
    uid_targets: Vec<UidTarget>,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
        .join("browser-memory")
}
fn path() -> PathBuf { data_dir().join("memory.json") }

fn load() -> Store {
    fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
fn save(store: &Store) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let tmp = data_dir().join("memory.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(store)?)?;
    fs::rename(tmp, path())?;
    Ok(())
}

pub fn domain_for_url(value: &str) -> String {
    url::Url::parse(value).ok()
        .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_ascii_lowercase()))
        .unwrap_or_default()
}

fn normalize(value: &str) -> String {
    value.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn remember_snapshot(url: &str, page_id: u32, snapshot: &str) -> Result<()> {
    let domain = domain_for_url(url);
    if domain.is_empty() { return Ok(()); }
    let re = Regex::new(r#"uid=([^\s]+)\s+([A-Za-z][A-Za-z0-9_-]*)(?:\s+\"([^\"]*)\")?"#)?;
    let now = Utc::now().to_rfc3339();
    let mut store = load();
    for cap in re.captures_iter(snapshot).take(400) {
        let uid = cap.get(1).map(|m|m.as_str()).unwrap_or("").to_string();
        let role = cap.get(2).map(|m|m.as_str()).unwrap_or("").to_string();
        let label = cap.get(3).map(|m|m.as_str()).unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
        if uid.is_empty() || label.is_empty() { continue; }
        if let Some(existing)=store.uid_targets.iter_mut().find(|x|x.uid==uid && x.domain==domain) {
            existing.page_id=page_id; existing.role=role; existing.label=label; existing.seen_at=now.clone();
        } else {
            store.uid_targets.push(UidTarget{uid,page_id,domain:domain.clone(),role,label,seen_at:now.clone()});
        }
    }
    if store.uid_targets.len()>800 {
        store.uid_targets.sort_by(|a,b|b.seen_at.cmp(&a.seen_at));
        store.uid_targets.truncate(800);
    }
    save(&store)
}

pub fn semantic_for_uid(uid: &str) -> Option<UidTarget> {
    let mut rows = load().uid_targets.into_iter().filter(|x|x.uid==uid).collect::<Vec<_>>();
    rows.sort_by(|a,b|b.seen_at.cmp(&a.seen_at));
    rows.into_iter().next()
}
pub fn semantic_for_uid_on_url(uid: &str, url: &str) -> Option<UidTarget> {
    let domain=domain_for_url(url);
    if domain.is_empty(){return semantic_for_uid(uid);}
    let mut rows=load().uid_targets.into_iter().filter(|x|x.uid==uid && x.domain==domain).collect::<Vec<_>>();
    rows.sort_by(|a,b|b.seen_at.cmp(&a.seen_at));
    rows.into_iter().next()
}

fn relation_score(candidate:&SemanticTarget,wanted:&str)->i64{
    let wanted=normalize(wanted);if wanted.is_empty(){return 0;}
    let mut values=vec![normalize(&candidate.label)];values.extend(candidate.aliases.iter().map(|x|normalize(x)));
    let wt=wanted.split_whitespace().collect::<std::collections::HashSet<_>>();
    let mut best=0i64;
    for value in values {
        if value.is_empty(){continue;}
        if value==wanted{best=best.max(1000);continue;}
        if value.contains(&wanted) || wanted.contains(&value){best=best.max(700);continue;}
        let vt=value.split_whitespace().collect::<std::collections::HashSet<_>>();
        let common=wt.intersection(&vt).count();let union=wt.union(&vt).count().max(1);
        let pct=(common*100/union) as i64;
        if pct>=50{best=best.max(300+pct);}
    }
    best
}

pub fn record_target(url: &str, action: &str, role: &str, label: &str, success: bool, requested_alias: Option<&str>) -> Result<()> {
    let domain = domain_for_url(url);
    let label = label.trim();
    if domain.is_empty() || label.is_empty() { return Ok(()); }
    let key_label = normalize(label);
    let now = Utc::now().to_rfc3339();
    let mut store=load();
    if let Some(target)=store.targets.iter_mut().find(|x|x.domain==domain && x.action==action && normalize(&x.label)==key_label) {
        target.role=role.to_string(); target.last_seen_at=now;
        if success { target.successes=target.successes.saturating_add(1); } else { target.failures=target.failures.saturating_add(1); }
        if let Some(alias)=requested_alias.map(str::trim).filter(|x|!x.is_empty()) {
            let a=normalize(alias);
            if a!=key_label && !target.aliases.iter().any(|x|normalize(x)==a) { target.aliases.push(alias.to_string()); }
            if target.aliases.len()>12 { target.aliases.drain(0..target.aliases.len()-12); }
        }
    } else {
        store.targets.push(SemanticTarget{
            domain,action:action.to_string(),role:role.to_string(),label:label.to_string(),
            aliases:requested_alias.filter(|x|normalize(x)!=key_label).map(|x|vec![x.to_string()]).unwrap_or_default(),
            successes:if success{1}else{0},failures:if success{0}else{1},last_seen_at:now,
        });
    }
    if store.targets.len()>500 {
        store.targets.sort_by(|a,b|b.last_seen_at.cmp(&a.last_seen_at)); store.targets.truncate(500);
    }
    save(&store)
}

pub fn candidates(url: &str, action: &str, wanted: &str, limit: usize) -> Vec<SemanticTarget> {
    let domain=domain_for_url(url); if domain.is_empty(){return Vec::new();}
    let mut rows=load().targets.into_iter()
        .filter(|x|x.domain==domain && x.action==action)
        .filter_map(|x|{let relation=relation_score(&x,wanted);if relation==0{None}else{Some((relation,x))}})
        .collect::<Vec<_>>();
    rows.sort_by_key(|(relation,x)|std::cmp::Reverse(*relation + (x.successes as i64*10) - (x.failures as i64*12)));
    rows.truncate(limit); rows.into_iter().map(|(_,x)|x).collect()
}

pub fn current_summary(url: &str, limit: usize) -> Value {
    let domain=domain_for_url(url);
    if domain.is_empty(){return json!({"domain":"","targets":[]});}
    let mut rows=load().targets.into_iter().filter(|x|x.domain==domain).collect::<Vec<_>>();
    rows.sort_by_key(|x|std::cmp::Reverse((x.successes as i64*10)-(x.failures as i64*12)));
    rows.truncate(limit);
    json!({"domain":domain,"targets":rows})
}

pub fn clear() -> Result<()> {
    let p=path(); if p.exists(){fs::remove_file(p)?;} Ok(())
}

pub fn stats() -> Value {
    let s=load();
    let mut domains:HashMap<String,usize>=HashMap::new();
    for t in &s.targets{*domains.entry(t.domain.clone()).or_default()+=1;}
    json!({"targets":s.targets.len(),"recent_uid_targets":s.uid_targets.len(),"domains":domains})
}
