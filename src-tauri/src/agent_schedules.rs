use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
};
use uuid::Uuid;

use crate::{credentials, models::AgentResponse, ollama::{self, SharedState}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSchedule {
    pub id: String,
    pub label: String,
    pub prompt: String,
    pub trigger_kind: String,
    pub trigger: String,
    pub browser_mode: String,
    #[serde(default)]
    pub credential_ids: Vec<String>,
    pub unit: String,
    pub created_at: String,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub last_status: Option<String>,
    #[serde(default)]
    pub last_result: Option<String>,
    #[serde(default)]
    pub last_session_id: Option<String>,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
}
fn path() -> PathBuf { data_dir().join("agent-schedules.json") }
fn unit_dir() -> PathBuf { dirs::home_dir().unwrap_or_default().join(".config/systemd/user") }
fn scripts_dir() -> PathBuf { data_dir().join("agent-schedules") }

fn load() -> Vec<AgentSchedule> {
    fs::read_to_string(path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default()
}
fn save(items:&[AgentSchedule]) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let tmp=data_dir().join("agent-schedules.json.tmp");
    fs::write(&tmp,serde_json::to_vec_pretty(items)?)?;
    fs::rename(tmp,path())?;
    Ok(())
}
fn clean_label(v:&str)->String { v.replace(['\r','\n']," ").trim().chars().take(120).collect() }
fn unit_quote(v:&str)->String { format!("\"{}\"",v.replace('\\',"\\\\").replace('"',"\\\"")) }

fn parse_delay_seconds(value:&str)->Result<i64> {
    let raw=value.trim().to_ascii_lowercase().replace(' ',"");
    let split=raw.find(|c:char|!c.is_ascii_digit()).unwrap_or(raw.len());
    let (digits,unit)=raw.split_at(split);
    let amount:i64=digits.parse().map_err(|_|anyhow!("Delay must look like 30s, 10m, 2h or 1d"))?;
    if amount<=0 { return Err(anyhow!("Delay must be greater than zero")); }
    let mult=match unit {
        ""|"s"|"sec"|"secs"|"second"|"seconds"=>1,
        "m"|"min"|"mins"|"minute"|"minutes"=>60,
        "h"|"hr"|"hrs"|"hour"|"hours"=>3600,
        "d"|"day"|"days"=>86400,
        _=>return Err(anyhow!("Unsupported delay unit")),
    };
    amount.checked_mul(mult).ok_or_else(||anyhow!("Delay is too large"))
}

fn run_systemctl(args:&[&str])->Result<()> {
    let out=Command::new("systemctl").args(["--user"]).args(args).output().context("Could not run systemctl --user")?;
    if !out.status.success() {
        return Err(anyhow!("systemctl --user failed: {}",String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

fn validate_credentials(ids:&[String])->Result<()> {
    let known=credentials::list()?
        .into_iter()
        .filter_map(|v|v.get("id").and_then(Value::as_str).map(str::to_string))
        .collect::<std::collections::HashSet<_>>();
    for id in ids {
        if !known.contains(id) { return Err(anyhow!("Unknown Fatir credential id: {id}")); }
    }
    Ok(())
}

fn write_units(item:&AgentSchedule, calendar:&str)->Result<(PathBuf,PathBuf,PathBuf)> {
    fs::create_dir_all(unit_dir())?;
    fs::create_dir_all(scripts_dir())?;
    fs::create_dir_all(data_dir().join("jobs"))?;

    let script=scripts_dir().join(format!("{}.sh",item.id));
    let service=unit_dir().join(format!("{}.service",item.unit));
    let timer=unit_dir().join(format!("{}.timer",item.unit));
    let log=data_dir().join("jobs").join(format!("agent-schedule-{}.log",item.id));

    let script_text=format!(r#"#!/usr/bin/env bash
set -u
exec >>{} 2>&1
echo "[Fatir agent schedule] starting $(date -Is)"
TOKEN_FILE="\${{XDG_DATA_HOME:-$HOME/.local/share}}/Fatir/companion/device-token"
if [[ ! -s "$TOKEN_FILE" ]]; then
  echo "Fatir companion token not found"
  exit 2
fi
TOKEN="$(cat "$TOKEN_FILE")"
HEALTH="http://127.0.0.1:32145/api/v1/health"
if ! curl -fsS --max-time 2 "$HEALTH" >/dev/null 2>&1; then
  if command -v fatir >/dev/null 2>&1; then
    (fatir --background >/dev/null 2>&1 &) || true
  fi
  for _ in $(seq 1 30); do
    curl -fsS --max-time 2 "$HEALTH" >/dev/null 2>&1 && break
    sleep 1
  done
fi
curl -fsS --max-time 1800 \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -X POST "http://127.0.0.1:32145/api/v1/agent/schedule/run?id={}" \
  || exit $?
echo
echo "[Fatir agent schedule] finished $(date -Is)"
"#, unit_quote(&log.display().to_string()), item.id);
    fs::write(&script,script_text)?;
    fs::set_permissions(&script,fs::Permissions::from_mode(0o700))?;

    let description=clean_label(&item.label);
    let service_text=format!(
        "[Unit]\nDescription=Fatir scheduled agent task: {}\n\n[Service]\nType=oneshot\nExecStart=/bin/bash {}\n",
        description, unit_quote(&script.display().to_string())
    );
    let timer_text=format!(
        "[Unit]\nDescription=Fatir agent timer: {}\n\n[Timer]\nOnCalendar={}\nPersistent=true\nUnit={}.service\n\n[Install]\nWantedBy=timers.target\n",
        description,calendar,item.unit
    );
    fs::write(&service,service_text)?;
    fs::write(&timer,timer_text)?;
    Ok((script,service,timer))
}

pub fn create(label:&str,prompt:&str,kind:&str,trigger:&str,browser_mode:&str,credential_ids:Vec<String>)->Result<Value> {
    if prompt.trim().is_empty(){return Err(anyhow!("Agent schedule prompt cannot be empty"));}
    if !matches!(kind,"delay"|"calendar"){return Err(anyhow!("trigger_kind must be delay or calendar"));}
    if trigger.trim().is_empty() || trigger.contains(['\n','\r']) { return Err(anyhow!("Schedule trigger must be one line")); }
    if !matches!(browser_mode,"none"|"managed"|"headless"|"current") { return Err(anyhow!("browser_mode must be none, managed, headless or current")); }
    validate_credentials(&credential_ids)?;

    let id=Uuid::new_v4().to_string();
    let unit=format!("fatir-agent-schedule-{}",&id[..8]);
    let calendar=if kind=="delay" {
        let seconds=parse_delay_seconds(trigger)?;
        (Local::now()+ChronoDuration::seconds(seconds)).format("%Y-%m-%d %H:%M:%S").to_string()
    } else { trigger.trim().to_string() };

    let item=AgentSchedule{
        id:id.clone(),
        label:clean_label(label),
        prompt:prompt.trim().chars().take(20_000).collect(),
        trigger_kind:kind.into(),
        trigger:trigger.trim().chars().take(200).collect(),
        browser_mode:browser_mode.into(),
        credential_ids,
        unit:unit.clone(),
        created_at:Utc::now().to_rfc3339(),
        last_run_at:None,last_status:None,last_result:None,last_session_id:None,
    };
    let (script,service,timer)=write_units(&item,&calendar)?;
    let timer_name=format!("{}.timer",unit);
    if let Err(err)=run_systemctl(&["daemon-reload"]).and_then(|_|run_systemctl(&["enable","--now",&timer_name])) {
        let _=fs::remove_file(timer); let _=fs::remove_file(service); let _=fs::remove_file(script); let _=run_systemctl(&["daemon-reload"]);
        return Err(err);
    }
    let mut items=load(); items.push(item.clone()); save(&items)?;
    Ok(json!({"scheduled":true,"job":item,"calendar":calendar,"timer":timer_name,"persistent_across_reboot":true}))
}

pub fn list()->Result<Vec<Value>> {
    let mut items=load();
    items.sort_by(|a,b|b.created_at.cmp(&a.created_at));
    Ok(items.into_iter().map(|x|{
        let timer=format!("{}.timer",x.unit);
        let state=Command::new("systemctl").args(["--user","is-active",&timer]).output().ok()
            .map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_else(||"unknown".into());
        let next=Command::new("systemctl").args(["--user","show",&timer,"-p","NextElapseUSecRealtime","--value"]).output().ok()
            .map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        json!({
            "id":x.id,"label":x.label,"prompt":x.prompt,"trigger_kind":x.trigger_kind,"trigger":x.trigger,
            "browser_mode":x.browser_mode,"credential_ids":x.credential_ids,"timer_state":state,"next":next,
            "created_at":x.created_at,"last_run_at":x.last_run_at,"last_status":x.last_status,
            "last_result":x.last_result,"last_session_id":x.last_session_id
        })
    }).collect())
}

pub fn cancel(id:&str)->Result<Value> {
    let mut items=load();
    let pos=items.iter().position(|x|x.id==id).ok_or_else(||anyhow!("Agent schedule not found"))?;
    let item=items[pos].clone();
    let timer_name=format!("{}.timer",item.unit);
    let service_name=format!("{}.service",item.unit);
    let _=Command::new("systemctl").args(["--user","disable","--now",&timer_name]).status();
    let _=Command::new("systemctl").args(["--user","stop",&service_name]).status();
    let _=fs::remove_file(unit_dir().join(&timer_name));
    let _=fs::remove_file(unit_dir().join(&service_name));
    let _=fs::remove_file(scripts_dir().join(format!("{}.sh",item.id)));
    let _=run_systemctl(&["daemon-reload"]);
    items.remove(pos); save(&items)?;
    Ok(json!({"cancelled":id}))
}

pub fn get(id:&str)->Result<AgentSchedule> {
    load().into_iter().find(|x|x.id==id).ok_or_else(||anyhow!("Agent schedule not found"))
}

fn execution_prompt(item:&AgentSchedule)->String {
    let browser=match item.browser_mode.as_str() {
        "managed"=>"This scheduled task was explicitly authorized to use Fatir's managed visible browser. Use browser tools as needed. Do not switch to headless.",
        "headless"=>"The user explicitly authorized this scheduled task to run headless at schedule creation. Use the headless browser when web work is needed.",
        "current"=>"This scheduled task was explicitly authorized to use the active/current user browser session only. Do not launch a separate managed browser.",
        _=>"This scheduled task has no browser authorization. Do not use browser or headless tools.",
    };
    format!(
        "FATIR SCHEDULED AGENT TASK\nSchedule: {}\n{}\nComplete the task end-to-end, verify consequential web actions, and report a concise result. If login requires CAPTCHA, authenticator/OTP, passkey, security key, or another human verification step, use browser_takeover and stop for the user instead of guessing or bypassing it.\n\nTask:\n{}",
        item.label,browser,item.prompt
    )
}

pub async fn run(state:SharedState,id:&str)->Result<Value> {
    let item=get(id)?;
    let session_id=format!("schedule-{}-{}", &item.id[..8], Utc::now().format("%Y%m%dT%H%M%SZ"));
    let grant=ollama::ExecutionGrant{
        source:"schedule".into(),
        schedule_id:Some(item.id.clone()),
        credential_ids:item.credential_ids.clone(),
    };
    let prompt=execution_prompt(&item);
    update_run(&item.id,"running",None,Some(&session_id))?;
    let response:Result<AgentResponse>=ollama::with_execution_grant(
        grant,
        ollama::send_message(state,&session_id,&prompt,Vec::new(),"auto")
    ).await;
    match response {
        Ok(r)=>{
            let status=if r.pending.is_some(){"awaiting_approval"} else if r.text.to_lowercase().contains("browser control is paused"){"awaiting_user"} else {"completed"};
            update_run(&item.id,status,Some(&r.text),Some(&session_id))?;
            Ok(json!({"ok":true,"schedule_id":item.id,"session_id":session_id,"status":status,"response":r}))
        }
        Err(e)=>{
            update_run(&item.id,"failed",Some(&e.to_string()),Some(&session_id))?;
            Err(e)
        }
    }
}

fn update_run(id:&str,status:&str,result:Option<&str>,session_id:Option<&str>)->Result<()> {
    let mut items=load();
    let item=items.iter_mut().find(|x|x.id==id).ok_or_else(||anyhow!("Agent schedule not found"))?;
    item.last_run_at=Some(Utc::now().to_rfc3339());
    item.last_status=Some(status.into());
    item.last_result=result.map(|x|x.chars().take(6000).collect());
    item.last_session_id=session_id.map(str::to_string);
    save(&items)
}
