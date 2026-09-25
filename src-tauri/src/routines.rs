use crate::memory;
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineStep {
    pub label: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub arguments: Option<Value>,
    #[serde(default)]
    pub surface: String,
    #[serde(default)]
    pub verify_with: Option<String>,
    #[serde(default = "default_replay_safe")]
    pub replay_safe: bool,
}

fn default_replay_safe() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<RoutineStep>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub run_count: u64,
}

fn data_dir() -> PathBuf { dirs::data_local_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir") }
fn path() -> PathBuf { data_dir().join("routines.json") }
fn load() -> Vec<Routine> { fs::read_to_string(path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default() }
fn save(items: &[Routine]) -> Result<()> { fs::create_dir_all(data_dir())?; let tmp=data_dir().join("routines.json.tmp"); fs::write(&tmp,serde_json::to_vec_pretty(items)?)?; fs::rename(tmp,path())?; Ok(()) }

fn clean_steps(steps: Vec<String>) -> Vec<RoutineStep> {
    steps.into_iter().filter(|s|!s.trim().is_empty()).take(40).map(|s|RoutineStep{label:s.trim().chars().take(500).collect(),tool:None,arguments:None,surface:"plan".into(),verify_with:None,replay_safe:true}).collect()
}

pub fn create(name:&str,description:&str,steps:Vec<String>)->Result<Value>{
    if name.trim().is_empty(){return Err(anyhow!("Routine name cannot be empty"));}
    let steps=clean_steps(steps); if steps.is_empty(){return Err(anyhow!("A routine needs at least one step"));}
    let now=Utc::now().to_rfc3339();
    let item=Routine{id:Uuid::new_v4().to_string(),name:name.trim().chars().take(120).collect(),description:description.trim().chars().take(1200).collect(),steps,created_at:now.clone(),updated_at:now,last_run_at:None,run_count:0};
    let mut items=load();items.push(item.clone());save(&items)?;Ok(serde_json::to_value(item)?)
}

pub fn create_recorded(name:&str,description:&str,mut steps:Vec<RoutineStep>)->Result<Value>{
    if name.trim().is_empty(){return Err(anyhow!("Routine name cannot be empty"));}
    steps.retain(|s|!s.label.trim().is_empty());
    steps.truncate(80);
    if steps.is_empty(){return Err(anyhow!("A recorded routine needs at least one reusable step"));}
    let now=Utc::now().to_rfc3339();
    let item=Routine{id:Uuid::new_v4().to_string(),name:name.trim().chars().take(120).collect(),description:description.trim().chars().take(1200).collect(),steps,created_at:now.clone(),updated_at:now,last_run_at:None,run_count:0};
    let mut items=load();items.push(item.clone());save(&items)?;Ok(serde_json::to_value(item)?)
}

pub fn capture_from_recent(name:&str,description:&str,action_count:usize)->Result<Value>{
    if name.trim().is_empty(){return Err(anyhow!("Routine name cannot be empty"));}
    let mut rows=memory::recent_actions(action_count.clamp(1,40))?;rows.reverse();
    let blocked=["browser_fill_credential","run_shell_with_credentials","credential_list","recent_actions","routine_summary","recent_activity"];
    let mut steps=Vec::new();
    for row in rows {
        if row.get("status").and_then(Value::as_str)!=Some("done"){continue;}
        let tool=row.get("tool").and_then(Value::as_str).unwrap_or("");
        if tool.is_empty()||blocked.contains(&tool){continue;}
        let args=row.get("arguments").cloned().unwrap_or_else(||json!({}));
        let arg_text=args.to_string();
        if arg_text.contains("[redacted]")||arg_text.contains("[not stored]"){continue;}
        steps.push(RoutineStep{label:format!("{} {}",tool.replace('_'," "),summarize_args(&args)),tool:Some(tool.to_string()),arguments:Some(args),surface:crate::orchestrator::surface_for_tool(tool).into(),verify_with:verification_hint(tool),replay_safe:true});
    }
    if steps.is_empty(){return Err(anyhow!("No reusable non-sensitive actions were found in the recent history"));}
    let now=Utc::now().to_rfc3339();let item=Routine{id:Uuid::new_v4().to_string(),name:name.trim().chars().take(120).collect(),description:description.trim().chars().take(1200).collect(),steps:steps.into_iter().take(30).collect(),created_at:now.clone(),updated_at:now,last_run_at:None,run_count:0};
    let mut items=load();items.push(item.clone());save(&items)?;Ok(serde_json::to_value(item)?)
}

fn verification_hint(tool:&str)->Option<String>{
    if crate::orchestrator::is_browser_mutation(tool){Some("browser_elements or browser_page_summary".into())}
    else if crate::orchestrator::is_desktop_mutation(tool){Some("desktop_wait_for, desktop_elements, or desktop_observe".into())}
    else{None}
}

fn summarize_args(v:&Value)->String{let s=v.to_string();if s.chars().count()>140{format!("{}…",s.chars().take(140).collect::<String>())}else{s}}

pub fn list()->Result<Vec<Value>>{let mut items=load();items.sort_by(|a,b|b.updated_at.cmp(&a.updated_at));Ok(items.into_iter().map(|x|serde_json::to_value(x).unwrap_or(json!({}))).collect())}
pub fn get(id:&str)->Result<Value>{let item=load().into_iter().find(|x|x.id==id).ok_or_else(||anyhow!("Routine not found"))?;Ok(serde_json::to_value(item)?) }
pub fn remove(id:&str)->Result<Value>{let mut items=load();let before=items.len();items.retain(|x|x.id!=id);if items.len()==before{return Err(anyhow!("Routine not found"));}save(&items)?;Ok(json!({"removed":id}))}
pub fn prepare_run(id:&str)->Result<Value>{let mut items=load();let item=items.iter_mut().find(|x|x.id==id).ok_or_else(||anyhow!("Routine not found"))?;item.run_count=item.run_count.saturating_add(1);item.last_run_at=Some(Utc::now().to_rfc3339());item.updated_at=Utc::now().to_rfc3339();let out=serde_json::to_value(item.clone())?;save(&items)?;Ok(out)}

pub fn context()->Result<String>{let mut items=load();if items.is_empty(){return Ok(String::new());}items.sort_by(|a,b|b.run_count.cmp(&a.run_count).then_with(||b.updated_at.cmp(&a.updated_at)));let compact:Vec<Value>=items.into_iter().take(8).map(|x|json!({"id":x.id,"name":x.name,"description":x.description,"run_count":x.run_count})).collect();Ok(format!("SAVED ROUTINES (use routine_prepare_run only when the user asks to run one): {}",serde_json::to_string(&compact)?))}
