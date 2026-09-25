use crate::{browser_memory, routines};
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TeachState {
    active: bool,
    name: String,
    description: String,
    started_at: String,
    #[serde(default)]
    steps: Vec<routines::RoutineStep>,
}

fn data_dir()->PathBuf{dirs::data_local_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir").join("teach")}
fn path()->PathBuf{data_dir().join("active.json")}
fn load()->TeachState{fs::read_to_string(path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default()}
fn save(s:&TeachState)->Result<()>{fs::create_dir_all(data_dir())?;let tmp=data_dir().join("active.json.tmp");fs::write(&tmp,serde_json::to_vec_pretty(s)?)?;fs::rename(tmp,path())?;Ok(())}

pub fn start(name:&str,description:&str)->Result<Value>{
    if name.trim().is_empty(){return Err(anyhow!("Teach mode needs a workflow name"));}
    let current=load(); if current.active{return Err(anyhow!("Teach mode is already recording '{}'",current.name));}
    let s=TeachState{active:true,name:name.trim().chars().take(120).collect(),description:description.trim().chars().take(1200).collect(),started_at:Utc::now().to_rfc3339(),steps:Vec::new()};
    save(&s)?;Ok(json!({"active":true,"name":s.name,"started_at":s.started_at,"message":"Teach Mode 3.0 is recording replay-safe semantic actions plus verification metadata. Credentials and secret values are never recorded."}))
}

pub fn status()->Value{let s=load();json!({"active":s.active,"name":s.name,"description":s.description,"started_at":s.started_at,"steps":s.steps.len()})}
pub fn is_active()->bool{load().active}

pub fn cancel()->Result<Value>{let s=load();if path().exists(){fs::remove_file(path())?;}Ok(json!({"cancelled":s.active,"name":s.name}))}

fn sensitive(tool:&str,args:&Value)->bool{
    if matches!(tool,"browser_fill_credential"|"desktop_fill_credential"|"run_shell_with_credentials"|"credential_list"){return true;}
    let s=args.to_string().to_lowercase();
    ["password","passwd","otp","one-time","recovery code","api_key","api-key","token","authorization","secret"].iter().any(|x|s.contains(x))
}

fn safe_text(value:&str)->String{
    if value.chars().count()>500 { format!("{}…",value.chars().take(500).collect::<String>()) } else { value.to_string() }
}

pub fn record(tool:&str,args:&Value,result:&str,status:&str)->Result<()> {
    let mut s=load(); if !s.active || status!="done" {return Ok(());}
    if tool.starts_with("teach_") || tool.starts_with("recent_") || tool.starts_with("routine_") || sensitive(tool,args){return Ok(());}
    // Observations, diagnostics, coordinate clicks and transient page IDs are not
    // replay-safe workflow steps. Teach Mode records stable actions, not debugging noise.
    if matches!(tool,
        "browser_observe"|"browser_elements"|"browser_page_summary"|"browser_extract_structure"|"browser_network_recent"|"browser_network_request"|"browser_console_messages"|"browser_diagnostics"|"browser_memory_current"|
        "browser_click"|"browser_tabs"|"browser_select_tab"|"browser_close_tab"|"browser_takeover"|"browser_takeover_resume"|"browser_takeover_status"|
        "headless_browser_observe"|"headless_browser_elements"|"headless_browser_extract_structure"|"headless_browser_network_recent"|"headless_browser_network_request"|"headless_browser_console_messages"|"headless_browser_tabs"|"headless_browser_select_tab"|"headless_browser_close_tab"|"headless_browser_stop") {return Ok(());}
    let mut out_tool=tool.to_string(); let mut out_args=args.clone(); let mut label=tool.replace('_'," ");
    match tool {
        "browser_click_element" => {
            let Some(uid)=args.get("element_id").and_then(Value::as_str) else{return Ok(());};
            let Some(t)=browser_memory::semantic_for_uid(uid) else{return Ok(());};
            out_tool="browser_click_text".into(); out_args=json!({"text":t.label,"exact":true}); label=format!("Click {} '{}'",t.role,t.label);
        }
        "browser_fill_element" => {
            let Some(uid)=args.get("element_id").and_then(Value::as_str) else{return Ok(());};
            let Some(t)=browser_memory::semantic_for_uid(uid) else{return Ok(());};
            let Some(text)=args.get("text").and_then(Value::as_str) else{return Ok(());};
            out_tool="browser_fill_by_label".into(); out_args=json!({"label":t.label,"text":safe_text(text)}); label=format!("Fill '{}'",t.label);
        }
        "browser_fill_by_label" => {
            if let Some(text)=out_args.get_mut("text") { if let Some(v)=text.as_str(){*text=Value::String(safe_text(v));} }
            label=format!("Fill '{}'",args.get("label").and_then(Value::as_str).unwrap_or("field"));
        }
        "browser_type" => {
            if let Some(text)=out_args.get_mut("text") { if let Some(v)=text.as_str(){*text=Value::String(safe_text(v));} }
            label="Type text into the focused browser field".into();
        }
        _=>{
            if let Ok(v)=serde_json::from_str::<Value>(result){
                if let Some(clicked)=v.get("clicked").and_then(Value::as_str){label=format!("{} '{}'",tool.replace('_'," "),clicked);}
            }
        }
    }
    if out_args.to_string().contains("[redacted]") || out_args.to_string().contains("[not stored]"){return Ok(());}
    let surface=crate::orchestrator::surface_for_tool(&out_tool).to_string();
    let verify_with=if crate::orchestrator::is_browser_mutation(&out_tool){Some("browser_elements or browser_page_summary".into())}else if crate::orchestrator::is_desktop_mutation(&out_tool){Some("desktop_wait_for, desktop_elements, or desktop_observe".into())}else{None};
    s.steps.push(routines::RoutineStep{label:label.chars().take(500).collect(),tool:Some(out_tool),arguments:Some(out_args),surface,verify_with,replay_safe:true});
    if s.steps.len()>80{s.steps.drain(0..s.steps.len()-80);}
    save(&s)
}

pub fn stop_and_save()->Result<Value>{
    let s=load(); if !s.active{return Err(anyhow!("Teach mode is not recording"));}
    if s.steps.is_empty(){return Err(anyhow!("Teach mode recorded no reusable actions"));}
    let routine=routines::create_recorded(&s.name,&s.description,s.steps.clone())?;
    if path().exists(){fs::remove_file(path())?;}
    Ok(json!({"saved":true,"routine":routine,"message":"Teach Mode 3.0 saved a replay-safe semantic workflow with verification hints. Browser targets are re-resolved on each run and GUI actions are never considered complete without state verification."}))
}
