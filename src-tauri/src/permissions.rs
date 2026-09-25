use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfig {
    #[serde(default)] pub confirm_file_changes: bool,
    #[serde(default)] pub confirm_shell_commands: bool,
    #[serde(default)] pub confirm_background_jobs: bool,
    #[serde(default)] pub confirm_automation_metadata: bool,
}
impl Default for PermissionConfig { fn default()->Self{Self{confirm_file_changes:false,confirm_shell_commands:false,confirm_background_jobs:false,confirm_automation_metadata:false}} }
fn data_dir()->PathBuf{dirs::data_local_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir")}
fn path()->PathBuf{data_dir().join("permissions.json")}
pub fn load()->PermissionConfig{fs::read_to_string(path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default()}
pub fn save(c:&PermissionConfig)->Result<()>{fs::create_dir_all(data_dir())?;let tmp=data_dir().join("permissions.json.tmp");fs::write(&tmp,serde_json::to_vec_pretty(c)?)?;fs::rename(tmp,path())?;Ok(())}

pub fn requires_approval(risk:&str,tool:&str)->bool{
    if risk=="auto"{return false;}
    if matches!(risk,"destructive"|"system"|"sensitive"){return true;}
    let c=load();
    if matches!(tool,"write_text_file"|"copy_file"|"move_path"|"create_directory"){return c.confirm_file_changes;}
    if matches!(tool,"background_job_start"|"background_job_cancel"|"scheduled_job_create"|"scheduled_job_cancel"){return c.confirm_background_jobs;}
    if matches!(tool,"routine_capture_recent"|"routine_remove"|"project_forget"|"terminal_session_close"){return c.confirm_automation_metadata;}
    if matches!(tool,"run_shell_command"|"terminal_session_exec"|"run_shell_with_credentials"){return c.confirm_shell_commands;}
    true
}

pub fn status()->Value{let c=load();json!({
    "config":c,
    "always_confirm":["destructive actions","privileged/system changes","stored credential use"],
    "never_bypass":"Delete/uninstall/erase/format, PolicyKit/root changes, credentials and purchases remain approval-gated regardless of preferences."
})}
