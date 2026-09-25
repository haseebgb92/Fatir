use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub label: String,
    pub command: String,
    pub cwd: String,
    pub trigger_kind: String,
    pub trigger: String,
    pub unit: String,
    pub created_at: String,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
}
fn path() -> PathBuf { data_dir().join("schedules.json") }
fn unit_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".config/systemd/user")
}
fn scripts_dir() -> PathBuf { data_dir().join("schedules") }
fn load() -> Vec<ScheduledJob> {
    fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
fn save(items: &[ScheduledJob]) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let tmp=data_dir().join("schedules.json.tmp");
    fs::write(&tmp,serde_json::to_vec_pretty(items)?)?;
    fs::rename(tmp,path())?;
    Ok(())
}
fn shell_quote(value:&str)->String { format!("'{}'",value.replace('\'',"'\\''")) }
fn unit_quote(value:&str)->String { format!("\"{}\"",value.replace('\\',"\\\\").replace('"',"\\\"")) }
fn clean_label(value:&str)->String { value.replace('\r'," ").replace('\n'," ").trim().chars().take(120).collect() }
fn expand(v:Option<&str>)->PathBuf {
    let v=v.unwrap_or("~");
    if v=="~" { dirs::home_dir().unwrap_or_default() }
    else if let Some(r)=v.strip_prefix("~/") { dirs::home_dir().unwrap_or_default().join(r) }
    else { PathBuf::from(v) }
}

fn parse_delay_seconds(value:&str)->Result<i64> {
    let raw=value.trim().to_ascii_lowercase();
    if raw.is_empty(){return Err(anyhow!("Delay cannot be empty"));}
    let compact=raw.replace(' ',"");
    let split=compact.find(|c:char|!c.is_ascii_digit()).unwrap_or(compact.len());
    let (digits,unit)=compact.split_at(split);
    let amount:i64=digits.parse().map_err(|_|anyhow!("Delay must look like 30s, 10m, 2h or 1d"))?;
    if amount<=0{return Err(anyhow!("Delay must be greater than zero"));}
    let multiplier=match unit {
        ""|"s"|"sec"|"secs"|"second"|"seconds"=>1,
        "m"|"min"|"mins"|"minute"|"minutes"=>60,
        "h"|"hr"|"hrs"|"hour"|"hours"=>3600,
        "d"|"day"|"days"=>86400,
        _=>return Err(anyhow!("Unsupported delay unit; use seconds, minutes, hours or days")),
    };
    amount.checked_mul(multiplier).ok_or_else(||anyhow!("Delay is too large"))
}

fn run_systemctl(args:&[&str])->Result<()> {
    let out=Command::new("systemctl").args(["--user"]).args(args).output().context("Could not run systemctl --user")?;
    if !out.status.success(){
        return Err(anyhow!("systemctl --user failed: {}",String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

fn write_units(item:&ScheduledJob, calendar:&str, log:&PathBuf)->Result<(PathBuf,PathBuf,PathBuf)> {
    fs::create_dir_all(unit_dir())?;
    fs::create_dir_all(scripts_dir())?;
    fs::create_dir_all(data_dir().join("jobs"))?;
    let script=scripts_dir().join(format!("{}.sh",item.id));
    let service=unit_dir().join(format!("{}.service",item.unit));
    let timer=unit_dir().join(format!("{}.timer",item.unit));

    let script_text=format!(
        "#!/usr/bin/env bash\ncd -- {} || exit $?\nexec >>{} 2>&1\necho '[Fatir schedule] started '$(date -Is)\n{}\ncode=$?\necho '[Fatir schedule] finished exit='$code' '$(date -Is)\nexit $code\n",
        shell_quote(&item.cwd), shell_quote(&log.display().to_string()), item.command
    );
    fs::write(&script,&script_text)?;
    fs::set_permissions(&script,fs::Permissions::from_mode(0o700))?;

    let description=clean_label(&item.label);
    let service_text=format!(
        "[Unit]\nDescription=Fatir scheduled job: {}\n\n[Service]\nType=oneshot\nExecStart=/bin/bash {}\n",
        description, unit_quote(&script.display().to_string())
    );
    let timer_text=format!(
        "[Unit]\nDescription=Fatir timer: {}\n\n[Timer]\nOnCalendar={}\nPersistent=true\nUnit={}.service\n\n[Install]\nWantedBy=timers.target\n",
        description, calendar, item.unit
    );
    fs::write(&service,service_text)?;
    fs::write(&timer,timer_text)?;
    Ok((script,service,timer))
}

pub fn create(label:&str,command:&str,cwd:Option<&str>,kind:&str,trigger:&str)->Result<Value>{
    if command.trim().is_empty(){return Err(anyhow!("Scheduled command cannot be empty"));}
    if !matches!(kind,"delay"|"calendar"){return Err(anyhow!("trigger_kind must be delay or calendar"));}
    if trigger.trim().is_empty(){return Err(anyhow!("Schedule trigger cannot be empty"));}
    if trigger.contains('\n') || trigger.contains('\r'){return Err(anyhow!("Schedule trigger must be one line"));}
    let dir=expand(cwd);if !dir.is_dir(){return Err(anyhow!("Working directory does not exist: {}",dir.display()));}
    let dir=fs::canonicalize(&dir).unwrap_or(dir);
    let id=Uuid::new_v4().to_string();
    let unit=format!("fatir-schedule-{}",&id[..8]);
    let calendar=if kind=="delay" {
        let seconds=parse_delay_seconds(trigger)?;
        let when=Local::now()+ChronoDuration::seconds(seconds);
        when.format("%Y-%m-%d %H:%M:%S").to_string()
    } else { trigger.trim().to_string() };
    let item=ScheduledJob{
        id:id.clone(),label:clean_label(label),command:command.to_string(),cwd:dir.display().to_string(),
        trigger_kind:kind.into(),trigger:trigger.trim().chars().take(200).collect(),unit:unit.clone(),created_at:Utc::now().to_rfc3339()
    };
    let log=data_dir().join("jobs").join(format!("schedule-{id}.log"));
    let (script,service,timer)=write_units(&item,&calendar,&log)?;

    let timer_name=format!("{}.timer",unit);
    if let Err(err)=run_systemctl(&["daemon-reload"]).and_then(|_|run_systemctl(&["enable","--now",timer_name.as_str()])) {
        let _=fs::remove_file(&timer);let _=fs::remove_file(&service);let _=fs::remove_file(&script);let _=run_systemctl(&["daemon-reload"]);
        return Err(err);
    }
    let mut items=load();items.retain(|x|x.id!=id);items.push(item.clone());items.sort_by(|a,b|b.created_at.cmp(&a.created_at));
    if let Err(err)=save(&items) {
        let _=Command::new("systemctl").args(["--user","disable","--now",timer_name.as_str()]).status();
        let _=fs::remove_file(&timer);let _=fs::remove_file(&service);let _=fs::remove_file(&script);let _=run_systemctl(&["daemon-reload"]);
        return Err(err.context("Could not persist Fatir schedule registry; timer was rolled back"));
    }
    Ok(json!({"scheduled":true,"job":item,"calendar":calendar,"timer":format!("{}.timer",unit),"persistent_across_reboot":true,"log":log}))
}

pub fn list()->Result<Vec<Value>>{
    let mut items=load();items.sort_by(|a,b|b.created_at.cmp(&a.created_at));
    Ok(items.into_iter().map(|x|{
        let timer=format!("{}.timer",x.unit);
        let state=Command::new("systemctl").args(["--user","is-active",&timer]).output().ok().map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_else(||"unknown".into());
        let enabled=Command::new("systemctl").args(["--user","is-enabled",&timer]).output().ok().map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_else(||"unknown".into());
        let next=Command::new("systemctl").args(["--user","show",&timer,"-p","NextElapseUSecRealtime","--value"]).output().ok().map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        json!({"id":x.id,"label":x.label,"cwd":x.cwd,"trigger_kind":x.trigger_kind,"trigger":x.trigger,"unit":x.unit,"timer_state":state,"enabled":enabled,"next":next,"created_at":x.created_at})
    }).collect())
}

pub fn cancel(id:&str)->Result<Value>{
    let mut items=load();let pos=items.iter().position(|x|x.id==id).ok_or_else(||anyhow!("Scheduled job not found"))?;let item=items[pos].clone();
    let timer_name=format!("{}.timer",item.unit);let service_name=format!("{}.service",item.unit);
    let _=Command::new("systemctl").args(["--user","disable","--now",&timer_name]).status();
    let _=Command::new("systemctl").args(["--user","stop",&service_name]).status();
    let _=Command::new("systemctl").args(["--user","reset-failed",&timer_name,&service_name]).status();
    let _=fs::remove_file(unit_dir().join(&timer_name));
    let _=fs::remove_file(unit_dir().join(&service_name));
    let _=fs::remove_file(scripts_dir().join(format!("{}.sh",item.id)));
    let _=run_systemctl(&["daemon-reload"]);
    items.remove(pos);save(&items)?;Ok(json!({"cancelled":id,"timer":timer_name}))
}
