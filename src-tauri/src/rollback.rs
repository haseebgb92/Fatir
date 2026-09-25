use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::{Path,PathBuf}, process::Command};
use uuid::Uuid;

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct UndoRecord{pub id:String,pub at:String,pub label:String,pub kind:String,pub data:Value,pub status:String}
fn data_dir()->PathBuf{dirs::data_local_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir")}
fn path()->PathBuf{data_dir().join("undo.json")}
fn load()->Vec<UndoRecord>{fs::read_to_string(path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default()}
fn save(items:&[UndoRecord])->Result<()>{fs::create_dir_all(data_dir())?;let tmp=data_dir().join("undo.json.tmp");fs::write(&tmp,serde_json::to_vec_pretty(items)?)?;fs::rename(tmp,path())?;Ok(())}
pub fn record(label:&str,kind:&str,data:Value)->Result<String>{let id=Uuid::new_v4().to_string();let mut items=load();items.push(UndoRecord{id:id.clone(),at:Utc::now().to_rfc3339(),label:label.into(),kind:kind.into(),data,status:"available".into()});if items.len()>300{let extra=items.len()-300;items.drain(0..extra);}save(&items)?;Ok(id)}
pub fn list(limit:usize)->Result<Vec<Value>>{let mut items=load();items.reverse();Ok(items.into_iter().take(limit.clamp(1,200)).map(|x|serde_json::to_value(x).unwrap_or(json!({}))).collect())}
pub fn snapshot_file(target:&Path,label:&str)->Result<Option<String>>{if !target.exists(){return Ok(Some(record(label,"remove_created",json!({"path":target.display().to_string()}))?));}if !target.is_file(){return Err(anyhow!("Automatic checkpoint currently supports files, not directories"));}let meta=fs::metadata(target)?;if meta.len()>100*1024*1024{return Err(anyhow!("Refusing to checkpoint a file larger than 100 MB"));}let dir=data_dir().join("rollback");fs::create_dir_all(&dir)?;let backup=dir.join(format!("{}-{}",Uuid::new_v4(),target.file_name().and_then(|x|x.to_str()).unwrap_or("backup")));fs::copy(target,&backup)?;Ok(Some(record(label,"restore_file",json!({"target":target.display().to_string(),"backup":backup.display().to_string()}))?))}
pub fn checkpoint(paths:&[String],label:&str)->Result<Vec<String>>{let mut ids=Vec::new();for p in paths.iter().take(12){let path=PathBuf::from(p);if let Some(id)=snapshot_file(&path,label)?{ids.push(id);}}Ok(ids)}
pub fn execute(id:&str)->Result<Value>{let mut items=load();let rec=items.iter_mut().find(|x|x.id==id).ok_or_else(||anyhow!("Undo record not found"))?;if rec.status!="available"{return Err(anyhow!("This rollback is no longer available"));}match rec.kind.as_str(){
"restore_file"=>{let target=rec.data.get("target").and_then(Value::as_str).ok_or_else(||anyhow!("Missing target"))?;let backup=rec.data.get("backup").and_then(Value::as_str).ok_or_else(||anyhow!("Missing backup"))?;if let Some(parent)=Path::new(target).parent(){fs::create_dir_all(parent)?;}fs::copy(backup,target).context("Restore failed")?;},
"remove_created"=>{let p=rec.data.get("path").and_then(Value::as_str).ok_or_else(||anyhow!("Missing path"))?;let path=Path::new(p);if path.is_file(){fs::remove_file(path)?;}else if path.is_dir(){fs::remove_dir(path).map_err(|e| anyhow!("Refusing to remove non-empty created directory during rollback: {e}"))?;}},
"move_back"=>{let from=rec.data.get("from").and_then(Value::as_str).ok_or_else(||anyhow!("Missing from"))?;let to=rec.data.get("to").and_then(Value::as_str).ok_or_else(||anyhow!("Missing to"))?;fs::rename(from,to)?;},
"apt_remove"=>{let pkg=rec.data.get("package").and_then(Value::as_str).ok_or_else(||anyhow!("Missing package"))?;let status=Command::new("pkexec").args(["apt-get","remove","-y",pkg]).status()?;if !status.success(){return Err(anyhow!("APT rollback failed"));}},
"flatpak_remove"=>{let app=rec.data.get("app_id").and_then(Value::as_str).ok_or_else(||anyhow!("Missing app id"))?;let status=Command::new("flatpak").args(["uninstall","--user","-y",app]).status()?;if !status.success(){return Err(anyhow!("Flatpak rollback failed"));}},
_=>return Err(anyhow!("Unsupported rollback kind")),}
rec.status="used".into();let out=serde_json::to_value(rec.clone())?;save(&items)?;Ok(out)}
