use anyhow::{anyhow, Result};
use chrono::Utc;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialMeta { pub id:String, pub label:String, pub account:String, pub created_at:String }
fn data_dir()->PathBuf{dirs::data_local_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir")}
fn path()->PathBuf{data_dir().join("credentials.json")}
fn load()->Vec<CredentialMeta>{fs::read_to_string(path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default()}
fn save(items:&[CredentialMeta])->Result<()>{fs::create_dir_all(data_dir())?;let tmp=data_dir().join("credentials.json.tmp");fs::write(&tmp,serde_json::to_vec_pretty(items)?)?;fs::rename(tmp,path())?;Ok(())}
fn service(id:&str)->String{format!("Fatir/credential/{id}")}
fn legacy_service(id:&str)->String{format!("AH/credential/{id}")}
fn key_user(account:&str)->&str{if account.trim().is_empty(){"default"}else{account.trim()}}
pub fn store(label:&str,account:&str,secret:&str)->Result<Value>{if label.trim().is_empty()||secret.is_empty(){return Err(anyhow!("Label and secret are required"));}let id=Uuid::new_v4().to_string();Entry::new(&service(&id),key_user(account))?.set_password(secret)?;let meta=CredentialMeta{id,label:label.trim().chars().take(120).collect(),account:account.trim().chars().take(180).collect(),created_at:Utc::now().to_rfc3339()};let mut items=load();items.push(meta.clone());save(&items)?;Ok(serde_json::to_value(meta)?) }
pub fn list()->Result<Vec<Value>>{Ok(load().into_iter().map(|x|serde_json::to_value(x).unwrap_or(Value::Null)).collect())}
pub fn remove(id:&str)->Result<()> {let mut items=load();let meta=items.iter().find(|x|x.id==id).cloned().ok_or_else(||anyhow!("Credential not found"))?;if let Ok(e)=Entry::new(&service(id),key_user(&meta.account)){let _=e.delete_credential();}if let Ok(e)=Entry::new(&legacy_service(id),key_user(&meta.account)){let _=e.delete_credential();}items.retain(|x|x.id!=id);save(&items)?;Ok(())}
pub fn secret(id:&str)->Result<String>{let meta=load().into_iter().find(|x|x.id==id).ok_or_else(||anyhow!("Credential not found"))?;if let Ok(e)=Entry::new(&service(id),key_user(&meta.account)){if let Ok(secret)=e.get_password(){return Ok(secret);}}let secret=Entry::new(&legacy_service(id),key_user(&meta.account))?.get_password()?;if let Ok(e)=Entry::new(&service(id),key_user(&meta.account)){let _=e.set_password(&secret);}Ok(secret)}
