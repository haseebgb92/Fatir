use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub text: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    pub at: String,
    pub label: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub status: String,
    #[serde(default = "default_phase")]
    pub phase: String,
    pub current_step: Option<String>,
    pub steps: Vec<TaskStep>,
    pub notes: Vec<String>,
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub checkpoints: Vec<TaskCheckpoint>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub resume_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

fn default_phase() -> String { "understand".into() }

fn data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir")
}
fn path() -> PathBuf { data_dir().join("tasks.json") }

fn load() -> Vec<Task> {
    fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
fn save(tasks: &[Task]) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let tmp = data_dir().join("tasks.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(tasks)?)?;
    fs::rename(tmp, path())?;
    Ok(())
}

pub fn create(title: &str, objective: &str, steps: Vec<String>) -> Result<Value> {
    if title.trim().is_empty() { return Err(anyhow!("Task title cannot be empty")); }
    let mut tasks = load();
    let now = Utc::now().to_rfc3339();
    let task = Task {
        id: Uuid::new_v4().to_string(),
        title: title.trim().chars().take(140).collect(),
        objective: objective.trim().chars().take(3000).collect(),
        status: "active".into(),
        phase: "understand".into(),
        current_step: steps.first().cloned(),
        steps: steps.into_iter().take(40).map(|text| TaskStep { text, status: "pending".into() }).collect(),
        notes: Vec::new(), artifacts: Vec::new(), checkpoints: Vec::new(), last_error: None, attempts: 0, resume_count: 0, created_at: now.clone(), updated_at: now,
    };
    tasks.push(task.clone()); save(&tasks)?;
    Ok(serde_json::to_value(task)?)
}

pub fn update(id: &str, status: Option<&str>, current_step: Option<&str>, completed_step: Option<&str>, note: Option<&str>, artifact: Option<&str>) -> Result<Value> {
    let mut tasks = load();
    let task = tasks.iter_mut().find(|t| t.id == id).ok_or_else(|| anyhow!("Task not found"))?;
    if let Some(s) = status {
        let allowed = ["active","waiting","blocked","completed","cancelled"];
        if !allowed.contains(&s) { return Err(anyhow!("Invalid task status")); }
        if s=="completed" && task.steps.len()>1 && !task.checkpoints.iter().any(|c|matches!(c.kind.as_str(),"verification"|"verified") && !c.evidence.trim().is_empty()){
            return Err(anyhow!("V1 multi-step task completion requires a verification checkpoint with evidence"));
        }
        task.status = s.into();
    }
    if let Some(step) = completed_step {
        if let Some(existing) = task.steps.iter_mut().find(|x| x.text == step) { existing.status = "done".into(); }
    }
    if let Some(step) = current_step {
        task.current_step = if step.trim().is_empty() { None } else { Some(step.trim().chars().take(500).collect()) };
        if !step.trim().is_empty() && !task.steps.iter().any(|x| x.text == step) {
            task.steps.push(TaskStep { text: step.trim().chars().take(500).collect(), status: "active".into() });
        }
        for s in &mut task.steps {
            if s.status == "active" && Some(s.text.as_str()) != task.current_step.as_deref() { s.status = "pending".into(); }
            if Some(s.text.as_str()) == task.current_step.as_deref() && s.status != "done" { s.status = "active".into(); }
        }
    }
    if let Some(n) = note { if !n.trim().is_empty() { task.notes.push(n.trim().chars().take(2000).collect()); } }
    if let Some(a) = artifact { if !a.trim().is_empty() && !task.artifacts.iter().any(|x| x == a) { task.artifacts.push(a.trim().to_string()); } }
    if task.status == "completed" { task.phase = "complete".into(); task.last_error = None; }
    else if task.status == "blocked" { task.phase = "recover".into(); }
    task.updated_at = Utc::now().to_rfc3339();
    let value = serde_json::to_value(task.clone())?; save(&tasks)?; Ok(value)
}

pub fn set_phase(id: &str, phase: &str) -> Result<Value> {
    let allowed=["understand","plan","act","verify","recover","waiting","complete"];
    if !allowed.contains(&phase){return Err(anyhow!("Invalid task phase"));}
    let mut tasks=load();let task=tasks.iter_mut().find(|t|t.id==id).ok_or_else(||anyhow!("Task not found"))?;
    if phase=="complete" {
        if task.steps.len()>1 && !task.checkpoints.iter().any(|c|matches!(c.kind.as_str(),"verification"|"verified") && !c.evidence.trim().is_empty()){
            return Err(anyhow!("V1 multi-step task completion requires a verification checkpoint with evidence"));
        }
        task.status="completed".into();task.last_error=None;
    }
    task.phase=phase.into();task.updated_at=Utc::now().to_rfc3339();let out=serde_json::to_value(task.clone())?;save(&tasks)?;Ok(out)
}

pub fn checkpoint(id:&str,label:&str,evidence:Option<&str>,kind:Option<&str>)->Result<Value>{
    if label.trim().is_empty(){return Err(anyhow!("Checkpoint label cannot be empty"));}
    let mut tasks=load();let task=tasks.iter_mut().find(|t|t.id==id).ok_or_else(||anyhow!("Task not found"))?;
    let checkpoint_kind=kind.unwrap_or("milestone").trim().chars().take(40).collect::<String>();
    let evidence_text=evidence.unwrap_or("").trim().chars().take(4000).collect::<String>();
    if matches!(checkpoint_kind.as_str(),"verification"|"verified") && evidence_text.is_empty(){return Err(anyhow!("Verification checkpoints require concrete evidence"));}
    task.checkpoints.push(TaskCheckpoint{at:Utc::now().to_rfc3339(),label:label.trim().chars().take(500).collect(),evidence:evidence_text,kind:checkpoint_kind.clone()});
    if checkpoint_kind=="verification" || checkpoint_kind=="verified" { task.phase="verify".into(); task.last_error=None; }
    if task.checkpoints.len()>80{task.checkpoints.drain(0..task.checkpoints.len()-80);}
    task.updated_at=Utc::now().to_rfc3339();let out=serde_json::to_value(task.clone())?;save(&tasks)?;Ok(out)
}

pub fn record_error(id:&str,error:&str)->Result<Value>{
    let mut tasks=load();let task=tasks.iter_mut().find(|t|t.id==id).ok_or_else(||anyhow!("Task not found"))?;
    task.last_error=Some(error.trim().chars().take(3000).collect());task.attempts=task.attempts.saturating_add(1);task.phase="recover".into();task.status="blocked".into();task.updated_at=Utc::now().to_rfc3339();
    let out=serde_json::to_value(task.clone())?;save(&tasks)?;Ok(out)
}

pub fn resume(id:&str)->Result<Value>{
    let mut tasks=load();let task=tasks.iter_mut().find(|t|t.id==id).ok_or_else(||anyhow!("Task not found"))?;
    task.resume_count=task.resume_count.saturating_add(1);task.status="active".into();if task.phase=="waiting"||task.phase=="recover"{task.phase="understand".into();}task.updated_at=Utc::now().to_rfc3339();
    let out=serde_json::to_value(task.clone())?;save(&tasks)?;Ok(out)
}

pub fn list(include_completed: bool) -> Result<Vec<Value>> {
    let mut tasks = load();
    tasks.sort_by(|a,b| b.updated_at.cmp(&a.updated_at));
    Ok(tasks.into_iter().filter(|t| include_completed || !matches!(t.status.as_str(), "completed"|"cancelled")).map(|t| serde_json::to_value(t).unwrap_or(json!({}))).collect())
}

pub fn get(id: &str) -> Result<Value> {
    let task = load().into_iter().find(|t| t.id == id).ok_or_else(|| anyhow!("Task not found"))?;
    Ok(serde_json::to_value(task)?)
}

pub fn active_context() -> Result<String> {
    let mut items=load();items.sort_by(|a,b|b.updated_at.cmp(&a.updated_at));
    let active: Vec<Task> = items.into_iter().filter(|t| matches!(t.status.as_str(), "active"|"waiting"|"blocked")).take(8).collect();
    if active.is_empty() { return Ok(String::new()); }
    let compact: Vec<Value> = active.into_iter().map(|t| json!({"id":t.id,"title":t.title,"status":t.status,"phase":t.phase,"current_step":t.current_step,"last_checkpoint":t.checkpoints.last().map(|c|c.label.clone()),"last_error":t.last_error,"updated_at":t.updated_at})).collect();
    Ok(format!("PERSISTENT TASK STATE (resume only when relevant): {}", serde_json::to_string(&compact)?))
}
