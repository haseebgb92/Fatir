use anyhow::Result;
use serde_json::{json, Value};
use std::{fs, process::Command};

fn run(cmd:&str,args:&[&str])->String{Command::new(cmd).args(args).output().ok().filter(|o|o.status.success()).map(|o|String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()}
fn appimages()->Vec<Value>{let mut out=Vec::new();let Some(home)=dirs::home_dir() else{return out};for dir in [home.join("Applications"),home.join("Downloads")]{if let Ok(rd)=fs::read_dir(dir){for e in rd.flatten(){let p=e.path();if p.extension().and_then(|x|x.to_str()).map(|x|x.eq_ignore_ascii_case("appimage")).unwrap_or(false){out.push(json!({"name":p.file_name().and_then(|x|x.to_str()).unwrap_or("AppImage"),"source":"appimage","path":p.display().to_string()}));}}}}out}
pub fn inventory(query:Option<&str>)->Result<Value>{let q=query.unwrap_or("").to_lowercase();let mut items=Vec::new();
let manual=run("apt-mark",&["showmanual"]);for name in manual.lines().filter(|x|!x.trim().is_empty()).take(2500){if !q.is_empty()&&!name.to_lowercase().contains(&q){continue;}let ver=run("dpkg-query",&["-W","-f=${Version}",name]).trim().to_string();items.push(json!({"name":name,"version":ver,"source":"apt"}));}
let flat=run("flatpak",&["list","--app","--columns=application,name,version"]);for line in flat.lines(){let c:Vec<&str>=line.split('\t').collect();if c.is_empty(){continue;}let id=c[0];let name=c.get(1).copied().unwrap_or(id);if !q.is_empty()&&!format!("{id} {name}").to_lowercase().contains(&q){continue;}items.push(json!({"name":name,"id":id,"version":c.get(2).copied().unwrap_or(""),"source":"flatpak"}));}
for x in appimages(){let name=x.get("name").and_then(Value::as_str).unwrap_or("");if q.is_empty()||name.to_lowercase().contains(&q){items.push(x);}}
Ok(json!({"count":items.len(),"items":items.into_iter().take(600).collect::<Vec<_>>() }))}
pub fn updates()->Result<Value>{let apt=run("bash",&["-lc","apt list --upgradable 2>/dev/null | sed 1d | head -n 250"]);let flat=run("flatpak",&["remote-ls","--updates","--columns=application,name,version"]);Ok(json!({"apt":apt.lines().collect::<Vec<_>>(),"flatpak":flat.lines().collect::<Vec<_>>() }))}
