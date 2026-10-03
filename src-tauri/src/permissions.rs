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
    matches!(risk,"install"|"uninstall"|"empty-trash") ||
    matches!(tool,
        "install_apt" | "install_flatpak" | "install_deb" | "desktop_repair_accessibility" |
        "uninstall_apt" | "uninstall_flatpak" | "uninstall_snap" |
        "empty_trash"
    )
}

pub fn status()->Value{let c=load();json!({
    "config":c,
    "always_confirm":["software installation","software uninstallation","permanently emptying Trash"],
    "autonomy":"All other supported Fatir actions run without redundant approval and still retain verification/rollback where available.",
    "never_bypass":"Installation, uninstallation and permanently emptying Trash always require explicit approval."
})}
