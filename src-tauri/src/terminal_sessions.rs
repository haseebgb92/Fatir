use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}};
use tokio::{process::Command, time::{timeout, Duration}};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionEntry { pub at:String,pub command:String,pub exit_code:i32,pub stdout:String,pub stderr:String }
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerminalSession { pub id:String,pub label:String,pub cwd:String,pub created_at:String,pub updated_at:String,#[serde(default)]pub history:Vec<SessionEntry> }

fn data_dir()->PathBuf{dirs::data_local_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir")}
fn path()->PathBuf{data_dir().join("terminal-sessions.json")}
fn load()->Vec<TerminalSession>{fs::read_to_string(path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default()}
fn save(items:&[TerminalSession])->Result<()>{fs::create_dir_all(data_dir())?;let tmp=data_dir().join("terminal-sessions.json.tmp");fs::write(&tmp,serde_json::to_vec_pretty(items)?)?;fs::rename(tmp,path())?;Ok(())}
fn expand(v:&str)->PathBuf{if v=="~"{dirs::home_dir().unwrap_or_default()}else if let Some(r)=v.strip_prefix("~/"){dirs::home_dir().unwrap_or_default().join(r)}else{PathBuf::from(v)}}

pub fn create(label:&str,cwd:Option<&str>)->Result<Value>{
    let dir=expand(cwd.unwrap_or("~"));if !dir.is_dir(){return Err(anyhow!("Working directory does not exist: {}",dir.display()));}
    let now=Utc::now().to_rfc3339();let s=TerminalSession{id:Uuid::new_v4().to_string(),label:if label.trim().is_empty(){"Terminal".into()}else{label.trim().chars().take(120).collect()},cwd:fs::canonicalize(&dir).unwrap_or(dir).display().to_string(),created_at:now.clone(),updated_at:now,history:Vec::new()};
    let mut items=load();items.push(s.clone());items.sort_by(|a,b|b.updated_at.cmp(&a.updated_at));items.truncate(30);save(&items)?;Ok(serde_json::to_value(s)?)
}

pub fn list()->Result<Vec<Value>>{let mut x=load();x.sort_by(|a,b|b.updated_at.cmp(&a.updated_at));Ok(x.into_iter().map(|s|json!({"id":s.id,"label":s.label,"cwd":s.cwd,"updated_at":s.updated_at,"commands":s.history.len()})).collect())}

pub fn set_cwd(id:&str,cwd:&str)->Result<Value>{let dir=expand(cwd);if !dir.is_dir(){return Err(anyhow!("Working directory does not exist: {}",dir.display()));}let mut items=load();let s=items.iter_mut().find(|s|s.id==id).ok_or_else(||anyhow!("Terminal session not found"))?;s.cwd=fs::canonicalize(dir).unwrap_or_else(|_|PathBuf::from(cwd)).display().to_string();s.updated_at=Utc::now().to_rfc3339();let out=serde_json::to_value(s.clone())?;save(&items)?;Ok(out)}

pub async fn exec(id:&str,command:&str,timeout_seconds:u64)->Result<Value>{
    if command.trim().is_empty(){return Err(anyhow!("Command cannot be empty"));}
    let mut items=load();let idx=items.iter().position(|s|s.id==id).ok_or_else(||anyhow!("Terminal session not found"))?;let cwd=items[idx].cwd.clone();
    let child=Command::new("/bin/bash").arg("-lc").arg(command).current_dir(&cwd).output();
    let out=timeout(Duration::from_secs(timeout_seconds.clamp(1,900)),child).await.map_err(|_|anyhow!("Terminal command timed out after {} seconds",timeout_seconds.clamp(1,900)))??;
    let stdout=String::from_utf8_lossy(&out.stdout).chars().take(20_000).collect::<String>();let stderr=String::from_utf8_lossy(&out.stderr).chars().take(12_000).collect::<String>();let code=out.status.code().unwrap_or(-1);
    let entry=SessionEntry{at:Utc::now().to_rfc3339(),command:command.chars().take(2000).collect(),exit_code:code,stdout:stdout.clone(),stderr:stderr.clone()};
    let s=&mut items[idx];s.history.push(entry);if s.history.len()>80{s.history.drain(0..s.history.len()-80);}s.updated_at=Utc::now().to_rfc3339();save(&items)?;
    Ok(json!({"session_id":id,"cwd":cwd,"exit_code":code,"success":out.status.success(),"stdout":stdout,"stderr":stderr}))
}

pub fn history(id:&str,limit:usize)->Result<Value>{let s=load().into_iter().find(|s|s.id==id).ok_or_else(||anyhow!("Terminal session not found"))?;let start=s.history.len().saturating_sub(limit.clamp(1,80));Ok(json!({"id":s.id,"label":s.label,"cwd":s.cwd,"history":s.history.into_iter().skip(start).collect::<Vec<_>>() }))}
pub fn close(id:&str)->Result<Value>{let mut items=load();let before=items.len();items.retain(|s|s.id!=id);if before==items.len(){return Err(anyhow!("Terminal session not found"));}save(&items)?;Ok(json!({"closed":id}))}

pub fn context()->Result<String>{let mut items=load();if items.is_empty(){return Ok(String::new());}items.sort_by(|a,b|b.updated_at.cmp(&a.updated_at));let c=items.into_iter().take(4).map(|s|json!({"id":s.id,"label":s.label,"cwd":s.cwd,"commands":s.history.len(),"updated_at":s.updated_at})).collect::<Vec<_>>();Ok(format!("PERSISTENT TERMINAL SESSIONS (use only when relevant): {}",serde_json::to_string(&c)?))}

#[allow(dead_code)] fn _is_dir(path:&Path)->bool{path.is_dir()}
