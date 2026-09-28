use crate::pointer;
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use tokio::process::Command;
use uuid::Uuid;

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
}

const DESKTOP_ACCESS_PACKAGES: &[&str] = &[
    // Semantic accessibility across GTK/Qt/Java desktop applications.
    "at-spi2-core",
    "python3-pyatspi",
    "python3-gi",
    "python3-cairo",
    "gir1.2-atspi-2.0",
    "gir1.2-gtk-3.0",
    "libatk-wrapper-java",
    "libatk-wrapper-java-jni",

    // X11 window/input/clipboard/visual fallbacks.
    "xdotool",
    "wmctrl",
    "xclip",
    "x11-utils",
    "xauth",
    "gnome-screenshot",
    "scrot",

    // Desktop launchers, D-Bus, privilege prompts and notifications.
    "xdg-utils",
    "desktop-file-utils",
    "libglib2.0-bin",
    "libgtk-3-bin",
    "dbus-x11",
    "libnotify-bin",
    "policykit-1",

    // Local file/application integration and secure credential service.
    "gvfs-backends",
    "shared-mime-info",
    "gnome-keyring",
    "flatpak",
    "poppler-utils",
    "unzip",
    "tar",
    "procps",
];

const LOCAL_RUNTIME_COMMANDS: &[(&str, &str)] = &[
    ("python3", "AT-SPI helper runtime"),
    ("wmctrl", "window discovery/focus"),
    ("xdotool", "keyboard and pointer fallback"),
    ("xclip", "clipboard/file handoff"),
    ("xprop", "window-class inspection"),
    ("gnome-screenshot", "primary screenshot capture"),
    ("scrot", "backup screenshot capture"),
    ("gio", "desktop launching/trash/file integration"),
    ("xdg-open", "default application/file opener"),
    ("gsettings", "desktop accessibility settings"),
    ("gdbus", "D-Bus/Secret Service diagnostics"),
    ("gtk-launch", "desktop launcher fallback"),
    ("update-desktop-database", "launcher database refresh"),
    ("notify-send", "desktop notifications"),
    ("pkexec", "PolicyKit privilege elevation"),
    ("flatpak", "Flatpak application discovery/install"),
    ("dpkg-query", "APT package state inspection"),
    ("dpkg-deb", "local DEB metadata inspection"),
    ("pdfinfo", "PDF metadata"),
    ("pdftotext", "PDF text extraction"),
    ("pdftoppm", "PDF page rendering"),
    ("unzip", "ZIP extraction"),
    ("tar", "archive extraction"),
    ("systemctl", "background job control"),
    ("systemd-run", "detached local jobs"),
    ("ps", "process inspection"),
    ("df", "disk inspection"),
    ("du", "storage inspection"),
    ("sha256sum", "file integrity"),
    ("git", "project/repository inspection"),
];


fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/applications"));
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    dirs
}

fn jetbrains_config_dirs() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else { return vec![]; };
    let mut roots = vec![home.join(".config/Google"), home.join(".config/JetBrains")];
    let mut out = Vec::new();
    for root in roots.drain(..) {
        let Ok(entries) = fs::read_dir(root) else { continue; };
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() { continue; }
            let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if ["androidstudio","intellijidea","pycharm","webstorm","clion","phpstorm","goland","datagrip","rubymine","rider","rustrover"].iter().any(|x| n.starts_with(x)) {
                out.push(p);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn jetbrains_vmoptions_target(dir: &std::path::Path) -> PathBuf {
    if let Ok(entries) = fs::read_dir(dir) {
        let mut existing = entries.flatten().map(|e|e.path()).filter(|p| {
            let n=p.file_name().and_then(|x|x.to_str()).unwrap_or("").to_ascii_lowercase();
            n.ends_with("64.vmoptions") || n.ends_with(".vmoptions")
        }).collect::<Vec<_>>();
        existing.sort();
        if let Some(p)=existing.into_iter().next(){ return p; }
    }
    let n=dir.file_name().and_then(|x|x.to_str()).unwrap_or("").to_ascii_lowercase();
    let file = if n.starts_with("androidstudio") { "studio64.vmoptions" }
        else if n.starts_with("pycharm") { "pycharm64.vmoptions" }
        else if n.starts_with("webstorm") { "webstorm64.vmoptions" }
        else if n.starts_with("clion") { "clion64.vmoptions" }
        else if n.starts_with("phpstorm") { "phpstorm64.vmoptions" }
        else if n.starts_with("goland") { "goland64.vmoptions" }
        else if n.starts_with("datagrip") { "datagrip64.vmoptions" }
        else if n.starts_with("rubymine") { "rubymine64.vmoptions" }
        else if n.starts_with("rider") { "rider64.vmoptions" }
        else if n.starts_with("rustrover") { "rustrover64.vmoptions" }
        else { "idea64.vmoptions" };
    dir.join(file)
}

async fn package_installed(package: &str) -> bool {
    Command::new("dpkg-query")
        .args(["-W", "-f=${Status}", package])
        .output()
        .await
        .ok()
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("install ok installed"))
        .unwrap_or(false)
}

fn append_unique_line(path: &std::path::Path, line: &str) -> Result<bool> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing.lines().any(|x| x.trim() == line) { return Ok(false); }
    if path.exists() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let name = path.file_name().and_then(|x|x.to_str()).unwrap_or("fatir-config");
        let backup = path.with_file_name(format!("{name}.fatir-backup-{stamp}"));
        let _ = fs::copy(path, backup);
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') { content.push('\n'); }
    content.push_str(line);
    content.push('\n');
    fs::write(path, content)?;
    Ok(true)
}

fn configure_jetbrains_accessibility() -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for dir in jetbrains_config_dirs() {
        let prop = dir.join("idea.properties");
        if append_unique_line(&prop, "ide.support.screenreaders.enabled=true")? {
            changed.push(prop.display().to_string());
        }
        let target = jetbrains_vmoptions_target(&dir);
        if append_unique_line(&target, "-Djavax.accessibility.assistive_technologies=org.GNOME.Accessibility.AtkWrapper")? {
            changed.push(target.display().to_string());
        }
    }
    Ok(changed)
}


const PY: &str = r#"
import json, sys, time
try:
    import pyatspi
except Exception as e:
    print(json.dumps({'error':'pyatspi unavailable: '+str(e)})); sys.exit(2)

def state_names(obj):
    try:
        ss=obj.getState(); return [pyatspi.stateToString(x) for x in ss.getStates()]
    except Exception:return []

def actions(obj):
    try:
        a=obj.queryAction(); return [a.getName(i) for i in range(a.nActions)]
    except Exception:return []

def bounds(obj):
    try:
        c=obj.queryComponent(); e=c.getExtents(pyatspi.DESKTOP_COORDS)
        return {'x':int(e.x),'y':int(e.y),'width':int(e.width),'height':int(e.height)}
    except Exception:return None

def windows():
    out=[]; desk=pyatspi.Registry.getDesktop(0)
    for ai in range(desk.childCount):
        app=desk.getChildAtIndex(ai)
        if not app: continue
        for wi in range(app.childCount):
            w=app.getChildAtIndex(wi)
            if not w: continue
            role=w.getRoleName(); name=(w.name or '').strip()
            if role in ('frame','dialog','window','application') or name:
                out.append({'app':(app.name or ''),'title':name,'role':role,'app_index':ai,'window_index':wi,'states':state_names(w)[:20],'bounds':bounds(w)})
    return out

def find_window(title):
    desk=pyatspi.Registry.getDesktop(0); needle=(title or '').lower().strip()
    for ai in range(desk.childCount):
        app=desk.getChildAtIndex(ai)
        if not app: continue
        for wi in range(app.childCount):
            w=app.getChildAtIndex(wi)
            if not w: continue
            name=(w.name or '')
            if (not needle) or needle in name.lower() or needle in (app.name or '').lower(): return w
    return None

def element_info(obj,path):
    role=''; name=''; desc=''
    try: role=obj.getRoleName() or ''
    except: pass
    try: name=(obj.name or '').strip()
    except: pass
    try: desc=(obj.description or '').strip()
    except: pass
    states=state_names(obj); acts=actions(obj)
    return {'path':'/'.join(map(str,path)),'role':role,'name':name[:240],'description':desc[:240],'actions':acts,'states':states[:20],'bounds':bounds(obj)}

def walk(root,limit=350):
    out=[]
    def rec(obj,path,depth):
        if len(out)>=limit or depth>10:return
        info=element_info(obj,path)
        useful = bool(info['name'] or info['description'] or info['actions'] or info['role'] in ('push button','check box','radio button','entry','text','combo box','menu item','page tab','link','slider','list item','table cell','tree item'))
        if useful: out.append(info)
        try:n=obj.childCount
        except:n=0
        for i in range(min(n,120)):
            try:child=obj.getChildAtIndex(i)
            except:continue
            if child:rec(child,path+[i],depth+1)
    rec(root,[],0); return out

def resolve(root,path):
    obj=root
    if path:
        for part in path.split('/'):
            if part=='':continue
            obj=obj.getChildAtIndex(int(part))
            if not obj: raise Exception('Accessibility element no longer exists; inspect the window again')
    return obj

def matches(items,query,role=''):
    q=(query or '').strip().lower(); rr=(role or '').strip().lower(); scored=[]
    for item in items:
        if rr and rr not in (item.get('role') or '').lower(): continue
        name=(item.get('name') or '').lower(); desc=(item.get('description') or '').lower(); combined=' '.join([name,desc,(item.get('role') or '').lower()])
        if not q: score=1
        elif name==q: score=100
        elif name.startswith(q): score=85
        elif q in name: score=70
        elif q in desc: score=55
        elif q in combined: score=40
        else: continue
        scored.append((score,item))
    scored.sort(key=lambda x:(-x[0],len(x[1].get('path') or '')))
    return [dict(item,match_score=score) for score,item in scored]

def perform_action(obj):
    preferred=['click','press','activate','toggle','open','select']
    a=obj.queryAction(); names=[a.getName(i).lower() for i in range(a.nActions)]
    idx=next((names.index(x) for x in preferred if x in names),0 if names else None)
    if idx is None: raise Exception('Element exposes no accessible action')
    ok=a.doAction(idx); return {'ok':bool(ok),'action':a.getName(idx),'name':obj.name or ''}

def main():
    op=sys.argv[1]
    if op=='windows': print(json.dumps({'windows':windows()})); return
    title=sys.argv[2] if len(sys.argv)>2 else ''
    w=find_window(title)
    if not w: print(json.dumps({'error':'Window not found'})); sys.exit(3)
    if op=='elements': print(json.dumps({'window':(w.name or ''),'elements':walk(w)})); return
    if op=='find':
        query=sys.argv[3] if len(sys.argv)>3 else ''; role=sys.argv[4] if len(sys.argv)>4 else ''
        print(json.dumps({'window':(w.name or ''),'query':query,'matches':matches(walk(w),query,role)[:25]})); return
    path=sys.argv[3] if len(sys.argv)>3 else ''
    if op=='info':
        try:
            obj=resolve(w,path); info=element_info(obj,[int(x) for x in path.split('/') if x!='']); print(json.dumps(info)); return
        except Exception as e: print(json.dumps({'error':str(e)})); sys.exit(4)
    if op=='activate':
        try: print(json.dumps(perform_action(resolve(w,path)))); return
        except Exception as e: print(json.dumps({'error':str(e)})); sys.exit(4)
    if op=='activate_name':
        query=path; role=sys.argv[4] if len(sys.argv)>4 else ''; occurrence=int(sys.argv[5]) if len(sys.argv)>5 and sys.argv[5].isdigit() else 0
        found=matches(walk(w),query,role)
        if occurrence<0 or occurrence>=len(found): print(json.dumps({'error':'No matching accessible element'})); sys.exit(4)
        try:
            result=perform_action(resolve(w,found[occurrence]['path'])); result['matched']=found[occurrence]; print(json.dumps(result)); return
        except Exception as e: print(json.dumps({'error':str(e)})); sys.exit(4)
    obj=resolve(w,path)
    if op in ('settext','setsecret'):
        text=sys.argv[4] if len(sys.argv)>4 else ''
        try:
            states=[x.lower() for x in state_names(obj)]
            if op=='settext' and 'protected' in states: raise Exception('Refusing to write a protected/password field through ordinary text automation')
            e=obj.queryEditableText(); e.setTextContents(text); print(json.dumps({'ok':True,'name':obj.name or '','protected':('protected' in states)})); return
        except Exception as e: print(json.dumps({'error':str(e)})); sys.exit(5)
    if op=='get_text':
        try:
            states=[x.lower() for x in state_names(obj)]
            if 'protected' in states: raise Exception('Protected/password text cannot be read')
            t=obj.queryText(); value=t.getText(0,t.characterCount); print(json.dumps({'name':obj.name or '','text':value[:12000]})); return
        except Exception as e: print(json.dumps({'error':str(e)})); sys.exit(5)
    print(json.dumps({'error':'Unknown operation'})); sys.exit(6)
main()
"#;

async fn py(args: &[&str]) -> Result<Value> {
    let out = Command::new("python3")
        .arg("-c")
        .arg(PY)
        .args(args)
        .output()
        .await
        .context("Could not run desktop accessibility helper")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let value: Value = serde_json::from_str(text.trim()).context("Desktop accessibility helper returned invalid data")?;
    if !out.status.success() {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Desktop accessibility operation failed")
            .to_string();
        return Err(anyhow!(message));
    }
    Ok(value)
}


fn desktop_entry_value<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    let mut in_main = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main { continue; }
        if let Some(v)=line.strip_prefix(&format!("{key}=")) { return Some(v.trim()); }
    }
    None
}

fn app_source(path: &std::path::Path) -> &'static str {
    let s=path.to_string_lossy();
    if s.contains("/flatpak/") { "flatpak" }
    else if s.contains("/snapd/") { "snap" }
    else if s.contains("/.local/") { "user" }
    else { "system" }
}

pub async fn apps(query: Option<&str>, limit: usize) -> Result<Value> {
    let q=query.unwrap_or("").trim().to_ascii_lowercase();
    let mut rows=Vec::<Value>::new();
    let mut seen=std::collections::HashSet::<String>::new();
    for dir in application_dirs() {
        let Ok(entries)=fs::read_dir(&dir) else { continue; };
        for entry in entries.flatten() {
            let path=entry.path();
            if path.extension().and_then(|x|x.to_str()) != Some("desktop") { continue; }
            let content=fs::read_to_string(&path).unwrap_or_default();
            if desktop_entry_value(&content,"Type").unwrap_or("Application") != "Application" { continue; }
            if desktop_entry_value(&content,"Hidden").map(|x|x.eq_ignore_ascii_case("true")).unwrap_or(false) { continue; }
            if desktop_entry_value(&content,"NoDisplay").map(|x|x.eq_ignore_ascii_case("true")).unwrap_or(false) { continue; }
            let name=desktop_entry_value(&content,"Name").unwrap_or("").trim();
            if name.is_empty() { continue; }
            let id=path.file_name().and_then(|x|x.to_str()).unwrap_or("").trim_end_matches(".desktop").to_string();
            let key=format!("{}::{}",name.to_ascii_lowercase(),id.to_ascii_lowercase());
            if !seen.insert(key) { continue; }
            let hay=format!("{} {} {}",name,id,desktop_entry_value(&content,"GenericName").unwrap_or("")).to_ascii_lowercase();
            if !q.is_empty() && !hay.contains(q.as_str()) { continue; }
            rows.push(json!({
                "name":name,
                "desktop_id":id,
                "source":app_source(&path),
                "path":path.display().to_string(),
                "categories":desktop_entry_value(&content,"Categories").unwrap_or(""),
                "terminal":desktop_entry_value(&content,"Terminal").map(|x|x.eq_ignore_ascii_case("true")).unwrap_or(false)
            }));
        }
    }
    rows.sort_by(|a,b|a.get("name").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase().cmp(&b.get("name").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase()));
    rows.truncate(limit.clamp(1,300));
    Ok(json!({"applications":rows,"query":query.unwrap_or(""),"sources":["system","user","flatpak","snap"]}))
}

async fn x11_windows() -> Vec<Value> {
    let Ok(out)=Command::new("wmctrl").arg("-lxG").output().await else { return vec![]; };
    if !out.status.success() { return vec![]; }
    let mut rows=Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let fields=line.split_whitespace().collect::<Vec<_>>();
        if fields.len()<9 { continue; }
        let parse_i=|i:usize| fields.get(i).and_then(|x|x.parse::<i64>().ok()).unwrap_or(0);
        rows.push(json!({
            "window_id":fields[0],
            "desktop":fields[1],
            "bounds":{"x":parse_i(2),"y":parse_i(3),"width":parse_i(4),"height":parse_i(5)},
            "host":fields[6],
            "wm_class":fields[7],
            "title":fields[8..].join(" "),
            "source":"x11"
        }));
    }
    rows
}

async fn x11_window_match(title: &str) -> Result<Value> {
    let needle=title.trim().to_ascii_lowercase();
    let rows=x11_windows().await;
    let mut scored=rows.into_iter().filter_map(|v|{
        let t=v.get("title").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
        let c=v.get("wm_class").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
        let score=if t==needle {100} else if c==needle {95} else if t.starts_with(needle.as_str()){85} else if t.contains(needle.as_str()){75} else if c.contains(needle.as_str()){65} else {0};
        (score>0).then_some((score,v))
    }).collect::<Vec<_>>();
    scored.sort_by(|a,b|b.0.cmp(&a.0));
    scored.into_iter().next().map(|(_,v)|v).ok_or_else(||anyhow!("Could not find an X11 window matching '{title}'"))
}

async fn command_available(name: &str) -> bool {
    let probe=format!("command -v {} >/dev/null 2>&1",name);
    Command::new("sh").args(["-lc",probe.as_str()]).status().await.map(|s|s.success()).unwrap_or(false)
}

pub async fn capabilities(window: &str) -> Result<Value> {
    let session=std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_|"unknown".into()).to_ascii_lowercase();
    let x11=x11_window_match(window).await.ok();
    let semantic=elements(window).await.ok();
    let elems=semantic.as_ref().and_then(|v|v.get("elements")).and_then(Value::as_array);
    let element_count=elems.map(|x|x.len()).unwrap_or(0);
    let actionable=elems.map(|xs|xs.iter().filter(|x|x.get("actions").and_then(Value::as_array).map(|a|!a.is_empty()).unwrap_or(false)).count()).unwrap_or(0);
    let editable=elems.map(|xs|xs.iter().filter(|x|{
        let role=x.get("role").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
        role.contains("entry") || role.contains("text")
    }).count()).unwrap_or(0);
    let xdotool=command_available("xdotool").await;
    let screenshot=command_available("gnome-screenshot").await || command_available("scrot").await;
    let semantic_ready=element_count>0;
    let pointer_fallback=session=="x11" && x11.is_some() && xdotool;
    let recommended=if semantic_ready {"semantic-at-spi"} else if pointer_fallback && screenshot {"visual-x11-fallback"} else if x11.is_some() && xdotool {"keyboard-window-fallback"} else {"limited"};
    Ok(json!({
        "window":window,
        "session_type":session,
        "semantic_atspi":semantic_ready,
        "semantic_elements":element_count,
        "actionable_elements":actionable,
        "editable_elements":editable,
        "window_control":x11.is_some(),
        "window_info":x11,
        "keyboard_fallback":xdotool,
        "visual_observation":screenshot,
        "coordinate_fallback":pointer_fallback,
        "recommended_control":recommended,
        "note":"Prefer semantic AT-SPI. Use keyboard next. Use visual X11 pointer fallback only when the application exposes no usable semantic control."
    }))
}

pub async fn doctor() -> Result<Value> {
    let mut packages = serde_json::Map::new();
    let mut missing = Vec::new();
    for package in DESKTOP_ACCESS_PACKAGES {
        let installed = package_installed(package).await;
        packages.insert((*package).to_string(), json!(installed));
        if !installed { missing.push((*package).to_string()); }
    }

    let mut runtime_commands = serde_json::Map::new();
    let mut missing_runtime_commands = Vec::new();
    for (command, purpose) in LOCAL_RUNTIME_COMMANDS {
        let available = command_available(command).await;
        runtime_commands.insert((*command).to_string(), json!({"available":available,"purpose":purpose}));
        if !available { missing_runtime_commands.push((*command).to_string()); }
    }

    let py_probe = Command::new("python3")
        .arg("-c")
        .arg("import pyatspi; d=pyatspi.Registry.getDesktop(0); print(d.childCount)")
        .output().await;
    let (python_atspi_ok, python_atspi_detail) = match py_probe {
        Ok(out) if out.status.success() => (true, format!("AT-SPI desktop reachable; {} top-level accessibility applications", String::from_utf8_lossy(&out.stdout).trim())),
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (false, if !err.is_empty() { err } else { stdout })
        }
        Err(err) => (false, err.to_string()),
    };

    let toolkit_accessibility = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "toolkit-accessibility"])
        .output().await.ok()
        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None });

    let jetbrains_dirs = jetbrains_config_dirs();
    let jetbrains_config = jetbrains_dirs.iter().map(|p| {
        let props = fs::read_to_string(p.join("idea.properties")).unwrap_or_default();
        let target = jetbrains_vmoptions_target(p);
        let vm = fs::read_to_string(&target).unwrap_or_default();
        json!({
            "product": p.file_name().and_then(|x|x.to_str()).unwrap_or("JetBrains"),
            "path": p.display().to_string(),
            "screen_reader_property": props.lines().any(|x|x.trim()=="ide.support.screenreaders.enabled=true"),
            "atk_vm_option": vm.lines().any(|x|x.trim()=="-Djavax.accessibility.assistive_technologies=org.GNOME.Accessibility.AtkWrapper"),
            "vmoptions": target.display().to_string()
        })
    }).collect::<Vec<_>>();

    let session_type=std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_|"unknown".into()).to_ascii_lowercase();
    let xdotool=command_available("xdotool").await;
    let wmctrl=command_available("wmctrl").await;
    let xclip=command_available("xclip").await;
    let screenshot=command_available("gnome-screenshot").await || command_available("scrot").await;
    let launcher_ready=command_available("gio").await && command_available("gtk-launch").await;
    let policykit_ready=command_available("pkexec").await;
    let notification_ready=command_available("notify-send").await;
    let desktop_database_ready=command_available("update-desktop-database").await;
    let session_bus_ready=std::env::var("DBUS_SESSION_BUS_ADDRESS").map(|x|!x.trim().is_empty()).unwrap_or(false);
    let secret_service_ready = if session_bus_ready && command_available("gdbus").await {
        Command::new("gdbus").args(["call","--session","--dest","org.freedesktop.secrets","--object-path","/org/freedesktop/secrets","--method","org.freedesktop.DBus.Peer.Ping"]).output().await.ok().map(|o|o.status.success()).unwrap_or(false)
    } else { false };
    let keyring_ready=package_installed("gnome-keyring").await && secret_service_ready;
    let x11_window_count=x11_windows().await.len();
    let semantic_ready = missing.iter().all(|x| !matches!(x.as_str(), "at-spi2-core"|"python3-pyatspi"|"python3-gi"|"gir1.2-atspi-2.0")) && python_atspi_ok;
    let x11_fallback_ready = session_type=="x11" && xdotool && wmctrl;
    let visual_fallback_ready = x11_fallback_ready && screenshot;
    let jetbrains_configured = jetbrains_config.iter().all(|v|
        v.get("screen_reader_property").and_then(Value::as_bool)==Some(true) &&
        v.get("atk_vm_option").and_then(Value::as_bool)==Some(true));
    let toolkit_enabled=toolkit_accessibility.as_deref()==Some("true");
    let runtime_ready = missing_runtime_commands.is_empty();
    let ready = semantic_ready && missing.is_empty() && runtime_ready;
    let universal_control_ready = semantic_ready && wmctrl && xdotool && xclip && screenshot && launcher_ready && policykit_ready;
    let repair_needed = !ready || !toolkit_enabled || (!jetbrains_config.is_empty() && !jetbrains_configured);
    Ok(json!({
        "ready": ready,
        "universal_control_ready": universal_control_ready,
        "semantic_atspi_ready": semantic_ready,
        "x11_fallback_ready": x11_fallback_ready,
        "visual_fallback_ready": visual_fallback_ready,
        "session_type":session_type,
        "visible_x11_windows":x11_window_count,
        "packages": packages,
        "missing_packages": missing,
        "runtime_commands": runtime_commands,
        "missing_runtime_commands": missing_runtime_commands,
        "runtime_commands_ready": runtime_ready,
        "clipboard_ready": xclip,
        "launcher_ready": launcher_ready,
        "policykit_ready": policykit_ready,
        "notification_ready": notification_ready,
        "desktop_database_ready": desktop_database_ready,
        "session_bus_ready": session_bus_ready,
        "secret_service_ready": secret_service_ready,
        "credential_keyring_ready": keyring_ready,
        "python_atspi_ok": python_atspi_ok,
        "python_atspi_detail": python_atspi_detail,
        "toolkit_accessibility": toolkit_accessibility,
        "jetbrains_configs": jetbrains_config,
        "repair_tool": if repair_needed { json!("desktop_repair_accessibility") } else { Value::Null },
        "control_order":["AT-SPI semantic controls","window-scoped keyboard","visual screenshot","X11 coordinate fallback with pointer restoration"],
        "note": "Fatir local runtime verifies semantic accessibility, X11 control, clipboard, screenshots, app launchers, PolicyKit, D-Bus, notifications, credential keyring, Flatpak/file integration, PDF/archive helpers and background-job commands. AT-SPI is preferred; X11 window/keyboard/visual fallbacks cover applications that expose little or no accessibility metadata. JetBrains-family apps may require a restart after their Java accessibility bridge is configured."
    }))
}

pub async fn repair_accessibility() -> Result<Value> {
    let package_list = DESKTOP_ACCESS_PACKAGES.join(" ");
    let command = format!("apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y {package_list}");
    let output = Command::new("pkexec")
        .arg("bash").arg("-lc").arg(&command)
        .output().await
        .context("Could not launch PolicyKit desktop-accessibility repair")?;
    if !output.status.success() {
        return Err(anyhow!("Desktop accessibility repair failed. stdout: {} stderr: {}",
            String::from_utf8_lossy(&output.stdout).trim(), String::from_utf8_lossy(&output.stderr).trim()));
    }

    let _ = Command::new("gsettings")
        .args(["set", "org.gnome.desktop.interface", "toolkit-accessibility", "true"])
        .status().await;

    let jetbrains_files = configure_jetbrains_accessibility()?;
    // Touch the bus once after installation; D-Bus normally autostarts AT-SPI.
    let _ = Command::new("python3")
        .arg("-c")
        .arg("import pyatspi; print(pyatspi.Registry.getDesktop(0).childCount)")
        .output().await;

    let mut report = doctor().await?;
    if let Some(obj) = report.as_object_mut() {
        obj.insert("changed_jetbrains_files".into(), json!(jetbrains_files));
        obj.insert("restart_jetbrains_apps".into(), json!(true));
        obj.insert("restart_note".into(), json!("Close and reopen any running JetBrains-family IDE after repair so its JVM loads the ATK accessibility bridge. Other GTK/Qt/Electron apps usually only need to be reopened if they were started before accessibility was enabled."));
    }
    Ok(report)
}

pub async fn key(window: &str, keys: &str) -> Result<Value> {
    if window.trim().is_empty() || keys.trim().is_empty() { return Err(anyhow!("Window and keys are required")); }
    focus_window(window).await?;
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let st = Command::new("xdotool").args(["key", "--clearmodifiers", keys]).status().await?;
    if !st.success() { return Err(anyhow!("Could not send keyboard shortcut '{keys}' to {window}")); }
    Ok(json!({"ok":true,"window":window,"keys":keys,"mouse_moved":false}))
}

pub async fn type_text(window: &str, text: &str) -> Result<Value> {
    if window.trim().is_empty() { return Err(anyhow!("Window is required")); }
    focus_window(window).await?;
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let st = Command::new("xdotool").args(["type", "--clearmodifiers", "--delay", "1", "--"]).arg(text).status().await?;
    if !st.success() { return Err(anyhow!("Could not type into {window}")); }
    Ok(json!({"ok":true,"window":window,"characters":text.chars().count(),"mouse_moved":false}))
}

pub async fn windows() -> Result<Value> {
    let semantic=py(&["windows"]).await.unwrap_or_else(|_|json!({"windows":[]}));
    let mut rows=semantic.get("windows").and_then(Value::as_array).cloned().unwrap_or_default();
    for row in rows.iter_mut() {
        if let Some(obj)=row.as_object_mut(){ obj.insert("source".into(),json!("atspi")); obj.insert("semantic".into(),json!(true)); }
    }
    for x in x11_windows().await {
        let title=x.get("title").and_then(Value::as_str).unwrap_or("");
        let class=x.get("wm_class").and_then(Value::as_str).unwrap_or("");
        let found=rows.iter_mut().find(|r|{
            let rt=r.get("title").and_then(Value::as_str).unwrap_or("");
            let app=r.get("app").and_then(Value::as_str).unwrap_or("");
            (!title.is_empty() && rt.eq_ignore_ascii_case(title)) || (!class.is_empty() && app.to_ascii_lowercase().contains(class.to_ascii_lowercase().as_str()))
        });
        if let Some(row)=found {
            if let Some(obj)=row.as_object_mut(){
                obj.insert("x11".into(),x.clone());
                obj.insert("window_id".into(),x.get("window_id").cloned().unwrap_or(Value::Null));
                obj.insert("wm_class".into(),x.get("wm_class").cloned().unwrap_or(Value::Null));
            }
        } else {
            rows.push(json!({
                "app":class,
                "title":title,
                "role":"window",
                "semantic":false,
                "source":"x11",
                "window_id":x.get("window_id").cloned().unwrap_or(Value::Null),
                "wm_class":class,
                "bounds":x.get("bounds").cloned().unwrap_or(Value::Null)
            }));
        }
    }
    Ok(json!({"windows":rows,"note":"Merged AT-SPI and X11 windows. semantic=false means use keyboard/visual fallback rather than assuming the app is uncontrollable."}))
}
pub async fn elements(window: &str) -> Result<Value> { py(&["elements", window]).await }
pub async fn find(window: &str, query: &str, role: Option<&str>) -> Result<Value> { py(&["find", window, query, role.unwrap_or("")]).await }
async fn point_at_path(window: &str, path: &str, label: &str) -> Result<()> {
    pointer::ensure_enabled()?;
    if let Ok(info)=py(&["info", window, path]).await {
        if let Some(bounds)=info.get("bounds") {
            pointer::move_to_bounds(bounds, label, "desktop")?;
            tokio::time::sleep(std::time::Duration::from_millis(380)).await;
        }
    }
    Ok(())
}

pub async fn activate(window: &str, path: &str) -> Result<Value> {
    point_at_path(window,path,"Fatir · Click").await?;
    let result=py(&["activate", window, path]).await?;
    Ok(result)
}
pub async fn activate_named(window: &str, query: &str, role: Option<&str>, occurrence: usize) -> Result<Value> {
    pointer::ensure_enabled()?;
    let found=find(window,query,role).await?;
    if let Some(hit)=found.get("matches").and_then(Value::as_array).and_then(|a|a.get(occurrence)) {
        if let Some(bounds)=hit.get("bounds") {
            let label=hit.get("name").and_then(Value::as_str).filter(|x|!x.is_empty()).unwrap_or(query);
            pointer::move_to_bounds(bounds,&format!("Fatir · {}",label.chars().take(24).collect::<String>()),"desktop")?;
            tokio::time::sleep(std::time::Duration::from_millis(420)).await;
        }
    }
    let occurrence = occurrence.to_string();
    py(&["activate_name", window, query, role.unwrap_or(""), &occurrence]).await
}
pub async fn set_text(window: &str, path: &str, text: &str) -> Result<Value> {
    point_at_path(window,path,"Fatir · Type").await?;
    py(&["settext", window, path, text]).await
}
pub async fn set_secret_text(window: &str, path: &str, text: &str) -> Result<Value> {
    point_at_path(window,path,"Fatir · Secure field").await?;
    py(&["setsecret", window, path, text]).await
}
pub async fn get_text(window: &str, path: &str) -> Result<Value> { py(&["get_text", window, path]).await }

pub async fn wait_for(window: &str, query: &str, role: Option<&str>, timeout_seconds: u64) -> Result<Value> {
    let timeout = timeout_seconds.clamp(1, 30);
    let started = std::time::Instant::now();
    loop {
        let result = find(window, query, role).await?;
        if result.get("matches").and_then(Value::as_array).map(|x| !x.is_empty()).unwrap_or(false) {
            return Ok(result);
        }
        if started.elapsed().as_secs() >= timeout { return Err(anyhow!("Timed out waiting for accessible element '{query}'")); }
        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
    }
}

pub async fn focus_window(title: &str) -> Result<Value> {
    if let Ok(info)=x11_window_match(title).await {
        if let Some(id)=info.get("window_id").and_then(Value::as_str) {
            let st=Command::new("wmctrl").args(["-ia",id]).status().await?;
            if st.success(){return Ok(json!({"ok":true,"window":title,"window_id":id,"matched_title":info.get("title")}));}
        }
    }
    let st = Command::new("wmctrl").args(["-a", title]).status().await?;
    if !st.success() { return Err(anyhow!("Could not focus window matching {title}")); }
    Ok(json!({"ok":true,"window":title}))
}

pub async fn launch_app(query: &str) -> Result<Value> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() { return Err(anyhow!("Application name cannot be empty")); }
    let mut best: Option<(i32, PathBuf, String, String)> = None;
    for dir in application_dirs() {
        let Ok(entries) = fs::read_dir(dir) else { continue; };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|x|x.to_str()) != Some("desktop") { continue; }
            let content = fs::read_to_string(&p).unwrap_or_default();
            if desktop_entry_value(&content,"Type").unwrap_or("Application") != "Application" { continue; }
            if desktop_entry_value(&content,"Hidden").map(|x|x.eq_ignore_ascii_case("true")).unwrap_or(false) { continue; }
            let filename = p.file_name().and_then(|x|x.to_str()).unwrap_or("");
            let id=filename.trim_end_matches(".desktop");
            let name = desktop_entry_value(&content,"Name").unwrap_or("");
            let generic=desktop_entry_value(&content,"GenericName").unwrap_or("");
            let hay = format!("{} {} {}", id, name, generic).to_ascii_lowercase();
            let score = if name.to_ascii_lowercase() == q { 100 }
                else if id.to_ascii_lowercase() == q { 98 }
                else if name.to_ascii_lowercase().starts_with(q.as_str()) { 85 }
                else if hay.contains(q.as_str()) { 60 } else { 0 };
            if score > best.as_ref().map(|x|x.0).unwrap_or(0) { best = Some((score, p.clone(), id.to_string(), name.to_string())); }
        }
    }
    let Some((_, path, desktop_id, name)) = best else { return Err(anyhow!("Could not find an installed desktop application matching '{query}'")); };
    let mut cmd=Command::new("gio");
    cmd.arg("launch").arg(&path);
    // These environment hints improve accessibility for many Qt/desktop apps launched by Fatir.
    cmd.env("QT_ACCESSIBILITY","1").env("ACCESSIBILITY_ENABLED","1");
    let st=cmd.status().await.context("Could not launch application with gio")?;
    if !st.success() {
        let st2=Command::new("gtk-launch").arg(&desktop_id).env("QT_ACCESSIBILITY","1").env("ACCESSIBILITY_ENABLED","1").status().await.context("Could not launch application with gtk-launch")?;
        if !st2.success(){ return Err(anyhow!("Could not launch {name} ({desktop_id})")); }
    }
    Ok(json!({"ok":true,"desktop_id":desktop_id,"name":name,"source":app_source(&path),"launcher":path.display().to_string()}))
}

async fn mouse_position() -> Option<(i64,i64)> {
    let out=Command::new("xdotool").args(["getmouselocation","--shell"]).output().await.ok()?;
    if !out.status.success(){return None;}
    let txt=String::from_utf8_lossy(&out.stdout);
    let mut x=None; let mut y=None;
    for line in txt.lines(){
        if let Some(v)=line.strip_prefix("X="){x=v.parse().ok();}
        if let Some(v)=line.strip_prefix("Y="){y=v.parse().ok();}
    }
    x.zip(y)
}

pub async fn visual_action(window:&str, action:&str, x:i64, y:i64, end_x:Option<i64>, end_y:Option<i64>, direction:Option<&str>, amount:u64, purpose:&str) -> Result<Value> {
    pointer::ensure_enabled()?;
    let session=std::env::var("XDG_SESSION_TYPE").unwrap_or_default().to_ascii_lowercase();
    if session!="x11" { return Err(anyhow!("Visual coordinate fallback is intentionally limited to X11. Use AT-SPI/keyboard control on this session.")); }
    let info=x11_window_match(window).await?;
    let b=info.get("bounds").ok_or_else(||anyhow!("Window geometry unavailable"))?;
    let wx=b.get("x").and_then(Value::as_i64).unwrap_or(0); let wy=b.get("y").and_then(Value::as_i64).unwrap_or(0);
    let ww=b.get("width").and_then(Value::as_i64).unwrap_or(1).max(1); let wh=b.get("height").and_then(Value::as_i64).unwrap_or(1).max(1);
    let rx=x.clamp(0,ww-1); let ry=y.clamp(0,wh-1); let ax=wx+rx; let ay=wy+ry;
    focus_window(window).await?;
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let original=mouse_position().await;
    pointer::move_to(ax,ay,&format!("Fatir · {}",purpose.chars().take(22).collect::<String>()),"desktop")?;
    tokio::time::sleep(std::time::Duration::from_millis(160)).await;
    let moved=Command::new("xdotool").arg("mousemove").arg("--sync").arg(ax.to_string()).arg(ay.to_string()).status().await?;
    if !moved.success(){return Err(anyhow!("Could not position the X11 fallback pointer in {window}"));}
    let op_result:Result<()> = async {
        match action {
            "click" => { if !Command::new("xdotool").args(["click","1"]).status().await?.success(){return Err(anyhow!("Visual click failed"));} }
            "double_click" => { if !Command::new("xdotool").args(["click","--repeat","2","--delay","90","1"]).status().await?.success(){return Err(anyhow!("Visual double-click failed"));} }
            "right_click" => { if !Command::new("xdotool").args(["click","3"]).status().await?.success(){return Err(anyhow!("Visual right-click failed"));} }
            "drag" => {
                let ex=end_x.ok_or_else(||anyhow!("drag requires end_x"))?.clamp(0,ww-1)+wx;
                let ey=end_y.ok_or_else(||anyhow!("drag requires end_y"))?.clamp(0,wh-1)+wy;
                if !Command::new("xdotool").args(["mousedown","1"]).status().await?.success(){return Err(anyhow!("Drag start failed"));}
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                let mv=Command::new("xdotool").arg("mousemove").arg("--sync").arg(ex.to_string()).arg(ey.to_string()).status().await.ok();
                let up=Command::new("xdotool").args(["mouseup","1"]).status().await.ok();
                if !mv.map(|s|s.success()).unwrap_or(false) || !up.map(|s|s.success()).unwrap_or(false){return Err(anyhow!("Visual drag failed"));}
            }
            "scroll" => {
                let button=match direction.unwrap_or("down").to_ascii_lowercase().as_str(){"up"=>"4","left"=>"6","right"=>"7",_=>"5"};
                let n=amount.clamp(1,30).to_string();
                if !Command::new("xdotool").args(["click","--repeat",&n,"--delay","35",button]).status().await?.success(){return Err(anyhow!("Visual scroll failed"));}
            }
            _ => return Err(anyhow!("Unsupported visual action '{action}'")),
        }
        Ok(())
    }.await;
    let restored=if let Some((ox,oy))=original { Command::new("xdotool").arg("mousemove").arg("--sync").arg(ox.to_string()).arg(oy.to_string()).status().await.map(|s|s.success()).unwrap_or(false) } else { false };
    let _=pointer::hide();
    op_result?;
    Ok(json!({"ok":true,"window":window,"action":action,"purpose":purpose,"relative":{"x":rx,"y":ry},"physical_pointer_temporarily_used":true,"physical_pointer_restored":restored,"fallback":"x11-visual"}))
}

pub async fn observe(window: Option<&str>) -> Result<Value> {
    if let Some(w)=window.filter(|x|!x.trim().is_empty()) {
        focus_window(w).await?;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    let dir = data_dir().join("Desktop");
    fs::create_dir_all(&dir)?;
    let p = dir.join(format!("desktop-{}.png", Uuid::new_v4()));
    let primary = if command_available("gnome-screenshot").await {
        Command::new("gnome-screenshot").args(["-w", "-f", p.to_string_lossy().as_ref()]).status().await.ok().map(|s|s.success()).unwrap_or(false)
    } else { false };
    let captured = if primary { true } else if command_available("scrot").await {
        Command::new("scrot").args(["-u", p.to_string_lossy().as_ref()]).status().await.ok().map(|s|s.success()).unwrap_or(false)
    } else { false };
    if !captured { return Err(anyhow!("Could not capture active window with gnome-screenshot or scrot")); }
    Ok(json!({"screenshot":p.display().to_string(),"window":window,"note":"Named/active window screenshot for visual fallback. Prefer semantic controls when available."}))
}

