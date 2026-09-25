use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub label: String,
    pub command: String,
    pub cwd: Option<String>,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub unit: String,
    pub log_path: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir")
}
fn path() -> PathBuf { data_dir().join("jobs.json") }
fn load() -> Vec<Job> { fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() }
fn save(jobs: &[Job]) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let tmp=data_dir().join("jobs.json.tmp");
    fs::write(&tmp,serde_json::to_vec_pretty(jobs)?)?;
    fs::rename(tmp,path())?;
    Ok(())
}
fn shell_quote(value:&str)->String { format!("'{}'", value.replace('\'', "'\\''")) }
fn expand_path(value:&str)->PathBuf { if value=="~" { dirs::home_dir().unwrap_or_default() } else if let Some(rest)=value.strip_prefix("~/") { dirs::home_dir().unwrap_or_default().join(rest) } else { PathBuf::from(value) } }
fn unit_state(unit:&str)->String {
    if unit.is_empty() { return "unknown".into(); }
    Command::new("systemctl").args(["--user","is-active",unit]).output().ok()
        .map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_else(||"unknown".into())
}
fn unit_pid(unit:&str)->u32 {
    Command::new("systemctl").args(["--user","show",unit,"-p","MainPID","--value"]).output().ok()
        .and_then(|o|String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0)
}
fn refresh(mut j:Job)->Job {
    if j.status=="running" {
        let state=unit_state(&j.unit);
        if state=="active" || state=="activating" { j.pid=unit_pid(&j.unit); }
        else { j.status=if state=="failed"{"failed".into()}else{"finished".into()}; j.updated_at=Utc::now().to_rfc3339(); }
    }
    j
}

pub fn start(label:&str, command:&str, cwd:Option<&str>) -> Result<Value> {
    if command.trim().is_empty(){return Err(anyhow!("Command cannot be empty"));}
    if Command::new("systemd-run").arg("--version").output().is_err(){return Err(anyhow!("systemd-run is unavailable; Fatir cannot safely detach this background job"));}
    fs::create_dir_all(data_dir().join("jobs"))?;
    let id=Uuid::new_v4().to_string();
    let short=&id[..8];
    let unit=format!("fatir-job-{short}.service");
    let log=data_dir().join("jobs").join(format!("{id}.log"));
    let workdir=cwd.map(expand_path).unwrap_or_else(||dirs::home_dir().unwrap_or_else(||PathBuf::from("/tmp")));
    if !workdir.is_dir(){return Err(anyhow!("Working directory does not exist: {}",workdir.display()));}
    let script=format!("exec >>{} 2>&1; echo '[Fatir] started '$(date -Is); {}; code=$?; echo '[Fatir] finished exit='$code' '$(date -Is); exit $code",shell_quote(&log.display().to_string()),command);
    let workdir_arg=format!("--working-directory={}",workdir.display());
    let out=Command::new("systemd-run")
        .args(["--user","--unit",&unit,"--collect",&workdir_arg,"/bin/bash","-lc",&script])
        .output().context("Could not start systemd user background job")?;
    if !out.status.success(){return Err(anyhow!("Could not start background job: {}",String::from_utf8_lossy(&out.stderr).trim()));}
    std::thread::sleep(std::time::Duration::from_millis(120));
    let pid=unit_pid(&unit);
    let now=Utc::now().to_rfc3339();
    let job=Job{id:id.clone(),label:label.trim().chars().take(120).collect(),command:command.to_string(),cwd:Some(workdir.display().to_string()),pid,unit,log_path:log.display().to_string(),status:"running".into(),created_at:now.clone(),updated_at:now};
    let mut jobs=load();jobs.push(job.clone());jobs.sort_by(|a,b|b.created_at.cmp(&a.created_at));save(&jobs)?;Ok(serde_json::to_value(job)?)
}

pub fn list()->Result<Vec<Value>>{
    let mut jobs:Vec<Job>=load().into_iter().map(refresh).collect();save(&jobs)?;jobs.sort_by(|a,b|b.created_at.cmp(&a.created_at));
    Ok(jobs.into_iter().map(|j|serde_json::to_value(j).unwrap_or(json!({}))).collect())
}
pub fn get(id:&str)->Result<Value>{
    let jobs:Vec<Job>=load().into_iter().map(refresh).collect();let j=jobs.iter().find(|j|j.id==id).ok_or_else(||anyhow!("Job not found"))?.clone();save(&jobs)?;Ok(serde_json::to_value(j)?)
}
pub fn log_tail(id:&str,lines:usize)->Result<String>{
    let j:Job=serde_json::from_value(get(id)?)?;let text=fs::read_to_string(j.log_path).unwrap_or_default();let rows:Vec<&str>=text.lines().collect();let start=rows.len().saturating_sub(lines.clamp(1,500));Ok(rows.into_iter().skip(start).collect::<Vec<_>>().join("\n"))
}
pub fn cancel(id:&str)->Result<Value>{
    let mut jobs=load();let j=jobs.iter_mut().find(|j|j.id==id).ok_or_else(||anyhow!("Job not found"))?;
    if j.status=="running" && !j.unit.is_empty(){let st=Command::new("systemctl").args(["--user","stop",&j.unit]).status()?;if !st.success(){return Err(anyhow!("Could not stop background job"));}}
    j.status="cancelled".into();j.updated_at=Utc::now().to_rfc3339();let v=serde_json::to_value(j.clone())?;save(&jobs)?;Ok(v)
}
