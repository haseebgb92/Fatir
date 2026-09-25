use crate::{cleanup, health, jobs};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, time::Duration};
use tokio::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveConfig {
    pub enabled: bool,
    pub desktop_notifications: bool,
    pub disk_warning_percent: u64,
    pub trash_warning_mb: u64,
    pub downloads_warning_mb: u64,
    pub scan_minutes: u64,
}
impl Default for ProactiveConfig { fn default()->Self{Self{enabled:true,desktop_notifications:true,disk_warning_percent:85,trash_warning_mb:1024,downloads_warning_mb:1536,scan_minutes:15}} }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveEvent {
    pub id:String,
    pub signature:String,
    pub kind:String,
    pub severity:String,
    pub title:String,
    pub message:String,
    pub action_prompt:String,
    pub created_at:String,
    #[serde(default)] pub acknowledged:bool,
}

fn data_dir()->PathBuf{dirs::data_local_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir")}
fn config_path()->PathBuf{data_dir().join("proactive-config.json")}
fn events_path()->PathBuf{data_dir().join("proactive-events.json")}
pub fn load_config()->ProactiveConfig{fs::read_to_string(config_path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default()}
pub fn save_config(c:&ProactiveConfig)->Result<()> {fs::create_dir_all(data_dir())?;fs::write(config_path(),serde_json::to_vec_pretty(c)?)?;Ok(())}
fn load_events()->Vec<ProactiveEvent>{fs::read_to_string(events_path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default()}
fn save_events(items:&[ProactiveEvent])->Result<()> {fs::create_dir_all(data_dir())?;let tmp=data_dir().join("proactive-events.json.tmp");fs::write(&tmp,serde_json::to_vec_pretty(items)?)?;fs::rename(tmp,events_path())?;Ok(())}

fn recent_duplicate(items:&[ProactiveEvent],signature:&str)->bool{
    if signature.starts_with("job:") { return items.iter().any(|e| e.signature == signature); }
    let cutoff=Utc::now()-ChronoDuration::hours(12);
    items.iter().any(|e|e.signature==signature&&DateTime::parse_from_rfc3339(&e.created_at).map(|d|d.with_timezone(&Utc)>=cutoff).unwrap_or(false))
}
async fn push_event(items:&mut Vec<ProactiveEvent>,cfg:&ProactiveConfig,kind:&str,severity:&str,title:&str,message:&str,prompt:&str,signature:&str){
    if recent_duplicate(items,signature){return;}
    let event=ProactiveEvent{id:Uuid::new_v4().to_string(),signature:signature.into(),kind:kind.into(),severity:severity.into(),title:title.into(),message:message.into(),action_prompt:prompt.into(),created_at:Utc::now().to_rfc3339(),acknowledged:false};
    if cfg.desktop_notifications { let _=Command::new("notify-send").args([title,message]).status().await; }
    items.push(event);if items.len()>120{let extra=items.len()-120;items.drain(0..extra);}
}

pub async fn run_once()->Result<Value>{
    let cfg=load_config();if !cfg.enabled{return Ok(json!({"enabled":false,"events_added":0}));}
    let mut events=load_events();let before=events.len();
    if let Ok(report)=health::report().await {
        let disk=report.get("disk_percent").and_then(Value::as_u64).unwrap_or(0);
        if disk>=cfg.disk_warning_percent {
            let sev=if disk>=92{"critical"}else{"warning"};let msg=format!("Root storage is {disk}% full.");
            push_event(&mut events,&cfg,"disk",sev,"Storage needs attention",&msg,"Review storage usage and cleanup candidates. Do not delete personal files without my approval.","disk-pressure").await;
        }
        let mem=report.get("memory_percent").and_then(Value::as_u64).unwrap_or(0);
        if mem>=94 { let msg=format!("Memory use reached {mem}%.");push_event(&mut events,&cfg,"memory","warning","Memory pressure",&msg,"Check memory pressure and top processes. Do not kill anything yet.","memory-pressure").await; }
    }
    if let Ok(trash)=cleanup::trash_summary(20) { let bytes=trash.get("bytes").and_then(Value::as_u64).unwrap_or(0); if bytes>=cfg.trash_warning_mb*1024*1024 { let size=trash.get("human").and_then(Value::as_str).unwrap_or("");let msg=format!("Trash currently contains about {size}.");push_event(&mut events,&cfg,"trash","info","Trash can free space",&msg,"Show me what is in Trash and whether I should empty it. Do not empty it yet.","trash-size").await; } }
    if let Ok(scan)=cleanup::scan(Some("downloads"),14,120) { let bytes=scan.get("high_confidence_reclaimable_bytes").and_then(Value::as_u64).unwrap_or(0); if bytes>=cfg.downloads_warning_mb*1024*1024 { let size=scan.get("high_confidence_reclaimable").and_then(Value::as_str).unwrap_or("");let msg=format!("Downloads has about {size} of high-confidence cleanup candidates.");push_event(&mut events,&cfg,"downloads","info","Downloads cleanup available",&msg,"Review my Downloads cleanup candidates and explain why each one is safe or uncertain. Do not delete anything yet.","downloads-cleanup").await; } }
    if let Ok(job_values)=jobs::list(){for j in job_values.into_iter().take(40){let status=j.get("status").and_then(Value::as_str).unwrap_or("");if !matches!(status,"finished"|"failed"){continue;}let id=j.get("id").and_then(Value::as_str).unwrap_or("");if id.is_empty(){continue;}let label=j.get("label").and_then(Value::as_str).unwrap_or("Background job");let sev=if status=="failed"{"warning"}else{"info"};let title=if status=="failed"{"Background job failed"}else{"Background job finished"};let msg=format!("{label} is {status}.");let prompt=format!("Check background job {id}, show me its final log, and explain the result.");push_event(&mut events,&cfg,"job",sev,title,&msg,&prompt,&format!("job:{id}:{status}")).await;}}
    save_events(&events)?;Ok(json!({"enabled":true,"events_added":events.len().saturating_sub(before),"unacknowledged":events.iter().filter(|e|!e.acknowledged).count()}))
}

pub fn events(limit:usize)->Result<Vec<Value>>{let mut items=load_events();items.retain(|x|!x.acknowledged);items.sort_by(|a,b|b.created_at.cmp(&a.created_at));Ok(items.into_iter().take(limit.clamp(1,100)).map(|x|serde_json::to_value(x).unwrap_or(json!({}))).collect())}
pub fn acknowledge(id:&str)->Result<Value>{let mut items=load_events();let e=items.iter_mut().find(|x|x.id==id).ok_or_else(||anyhow!("Proactive event not found"))?;e.acknowledged=true;let out=serde_json::to_value(e.clone())?;save_events(&items)?;Ok(out)}
pub fn status()->Value{json!({"config":load_config(),"unacknowledged":load_events().iter().filter(|x|!x.acknowledged).count()})}
pub async fn monitor_forever(){loop{let cfg=load_config();if cfg.enabled{let _=run_once().await;}tokio::time::sleep(Duration::from_secs(cfg.scan_minutes.clamp(5,240)*60)).await;}}
