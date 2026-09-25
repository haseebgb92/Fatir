use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurn {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub model: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatSessionSummary {
    pub session_id: String,
    pub title: String,
    pub preview: String,
    pub updated_at: String,
    pub messages: usize,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
}
fn path() -> PathBuf { data_dir().join("chat-history.json") }

fn load() -> Vec<ChatTurn> {
    fs::read_to_string(path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(items: &[ChatTurn]) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let tmp=data_dir().join("chat-history.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(items)?)?;
    fs::rename(tmp,path())?;
    Ok(())
}

pub fn append(session_id:&str, role:&str, text:&str, model:Option<&str>) -> Result<()> {
    let text=text.trim();
    if text.is_empty() { return Ok(()); }
    let mut items=load();
    items.push(ChatTurn{
        id:Uuid::new_v4().to_string(),
        session_id:session_id.to_string(),
        role:role.to_string(),
        text:text.chars().take(60_000).collect(),
        model:model.map(|s|s.to_string()),
        created_at:Utc::now().to_rfc3339(),
    });
    if items.len()>8000 {
        let drain=items.len()-8000;
        items.drain(0..drain);
    }
    save(&items)
}

pub fn sessions(limit:usize) -> Vec<ChatSessionSummary> {
    use std::collections::HashMap;
    #[derive(Default)]
    struct Acc { first_user:Option<String>, last:String, updated:String, count:usize }
    let mut map:HashMap<String,Acc>=HashMap::new();
    for item in load() {
        if item.role!="user" && item.role!="assistant" { continue; }
        let acc=map.entry(item.session_id.clone()).or_default();
        if item.role=="user" && acc.first_user.is_none() { acc.first_user=Some(item.text.clone()); }
        acc.last=item.text.clone();
        acc.updated=item.created_at.clone();
        acc.count+=1;
    }
    let mut out=map.into_iter().map(|(session_id,a)| {
        let title=a.first_user.unwrap_or_else(||"Fatir chat".into());
        ChatSessionSummary{
            session_id,
            title:short(&title,72),
            preview:short(&a.last,120),
            updated_at:a.updated,
            messages:a.count,
        }
    }).collect::<Vec<_>>();
    out.sort_by(|a,b|b.updated_at.cmp(&a.updated_at));
    out.truncate(limit.clamp(1,200));
    out
}

pub fn messages(session_id:&str, limit:usize) -> Vec<ChatTurn> {
    let mut items=load().into_iter()
        .filter(|x|x.session_id==session_id && (x.role=="user" || x.role=="assistant"))
        .collect::<Vec<_>>();
    let keep=limit.clamp(1,1000);
    if items.len()>keep { items.drain(0..items.len()-keep); }
    items
}

fn short(s:&str,n:usize)->String {
    let mut out=s.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.chars().count()>n { out=out.chars().take(n.saturating_sub(1)).collect::<String>()+"…"; }
    out
}
