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

const SUDO_SERVICE: &str = "Fatir";
const SUDO_ACCOUNT: &str = "sudo_password";
const LEGACY_SUDO_SERVICE: &str = "Fatir/admin";
const LEGACY_SUDO_ACCOUNT: &str = "sudo";

pub fn store_sudo(secret:&str)->Result<Value>{
    if secret.is_empty(){return Err(anyhow!("Administrator password is required"));}
    let entry=Entry::new(SUDO_SERVICE,SUDO_ACCOUNT)?;
    entry.set_password(secret)?;
    // Verify the same Secret Service slot can be read back before claiming success.
    let verified=entry.get_password().map(|v|!v.is_empty()).unwrap_or(false);
    if !verified { return Err(anyhow!("Administrator password was written but could not be read back from Linux keyring")); }
    // Remove the temporary v1.2.0 legacy slot if it exists.
    if let Ok(old)=Entry::new(LEGACY_SUDO_SERVICE,LEGACY_SUDO_ACCOUNT){let _=old.delete_credential();}
    Ok(serde_json::json!({"stored":true,"verified":true,"service":"linux-keyring","kind":"sudo"}))
}

pub fn sudo_secret()->Result<String>{
    if let Ok(entry)=Entry::new(SUDO_SERVICE,SUDO_ACCOUNT){
        if let Ok(secret)=entry.get_password(){
            if !secret.is_empty(){return Ok(secret);}
        }
    }
    // Migrate any password saved by the first 1.2.0 build.
    let old=Entry::new(LEGACY_SUDO_SERVICE,LEGACY_SUDO_ACCOUNT)?.get_password()?;
    if old.is_empty(){return Err(anyhow!("Administrator password is not stored"));}
    Entry::new(SUDO_SERVICE,SUDO_ACCOUNT)?.set_password(&old)?;
    if let Ok(legacy)=Entry::new(LEGACY_SUDO_SERVICE,LEGACY_SUDO_ACCOUNT){let _=legacy.delete_credential();}
    Ok(old)
}

pub fn has_sudo()->bool{sudo_secret().map(|s|!s.is_empty()).unwrap_or(false)}

pub fn remove_sudo()->Result<()> {
    if let Ok(e)=Entry::new(SUDO_SERVICE,SUDO_ACCOUNT){let _=e.delete_credential();}
    if let Ok(e)=Entry::new(LEGACY_SUDO_SERVICE,LEGACY_SUDO_ACCOUNT){let _=e.delete_credential();}
    Ok(())
}
