use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::json;
use std::{fs, io::Write, os::unix::net::UnixStream, path::PathBuf, process::{Command, Stdio}, sync::atomic::{AtomicBool, AtomicU8, Ordering}, thread, time::Duration};

static PAUSED: AtomicBool = AtomicBool::new(false);
// 0 = hidden/idle, 1 = desktop overlay, 2 = browser overlay. Only one surface owns the Fatir pointer at a time.
static SURFACE: AtomicU8 = AtomicU8::new(0);

#[derive(Serialize)]
pub struct ComputerControlState {
    pub paused: bool,
    pub pointer: &'static str,
    pub physical_mouse_moved: bool,
    pub surface: &'static str,
}

pub fn status() -> ComputerControlState {
    ComputerControlState {
        paused: PAUSED.load(Ordering::Relaxed),
        pointer: "Fatir virtual pointer",
        physical_mouse_moved: false,
        surface: match SURFACE.load(Ordering::Relaxed) { 1 => "desktop", 2 => "browser", _ => "hidden" },
    }
}

pub fn set_paused(paused: bool) -> ComputerControlState {
    PAUSED.store(paused, Ordering::Relaxed);
    if paused {
        SURFACE.store(0, Ordering::Relaxed);
        let _ = send_if_running(json!({"visible":false}));
    }
    status()
}

pub fn ensure_enabled() -> Result<()> {
    if PAUSED.load(Ordering::Relaxed) {
        return Err(anyhow!("Fatir computer control is paused. Resume it from the Fatir panel before continuing."));
    }
    Ok(())
}

fn socket_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
        .join("pointer.sock")
}

const OVERLAY_PY: &str = r#"
import gi, json, math, os, socket, sys, threading

gi.require_version('Gtk','3.0')
from gi.repository import Gtk, Gdk, GLib
import cairo

sock_path=sys.argv[1]
parent=int(sys.argv[2]) if len(sys.argv)>2 else 0

class Pointer(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.POPUP)
        self.set_decorated(False)
        self.set_app_paintable(True)
        self.set_keep_above(True)
        self.set_accept_focus(False)
        self.set_focus_on_map(False)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        self.set_default_size(240,76)
        self.resize(240,76)
        screen=self.get_screen()
        visual=screen.get_rgba_visual() if screen else None
        if visual: self.set_visual(visual)
        self.connect('draw', self.draw)
        self.connect('realize', self.on_realize)
        self.x=120.0; self.y=120.0
        self.tx=self.x; self.ty=self.y
        self.label='Fatir'; self.state='control'; self.alpha=1.0
        self.anim_source=None
        self.hide_source=None
        self.hide()

    def on_realize(self,*_):
        try:
            win=self.get_window()
            if hasattr(win,'set_pass_through'): win.set_pass_through(True)
        except Exception: pass

    def draw(self, widget, cr):
        cr.set_operator(cairo.OPERATOR_SOURCE)
        cr.set_source_rgba(0,0,0,0); cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)
        # Cursor arrow with a bright edge so it remains readable on light/dark apps.
        cr.move_to(13,8); cr.line_to(13,36); cr.line_to(20,29); cr.line_to(26,43); cr.line_to(33,39); cr.line_to(27,26); cr.line_to(38,25); cr.close_path()
        cr.set_source_rgba(1,1,1,.98); cr.set_line_width(4); cr.stroke_preserve()
        cr.set_source_rgba(.10,.54,.43,.98); cr.fill()
        # Compact status bubble.
        label=self.label[:28]
        if label:
            cr.set_source_rgba(.08,.10,.12,.90)
            x,y,w,h=39,12,min(188,max(58,10+len(label)*6.4)),29
            r=10
            cr.new_sub_path(); cr.arc(x+w-r,y+r,r,-math.pi/2,0); cr.arc(x+w-r,y+h-r,r,0,math.pi/2); cr.arc(x+r,y+h-r,r,math.pi/2,math.pi); cr.arc(x+r,y+r,r,math.pi,3*math.pi/2); cr.close_path(); cr.fill()
            cr.set_source_rgba(1,1,1,.98); cr.select_font_face('Sans',cairo.FONT_SLANT_NORMAL,cairo.FONT_WEIGHT_BOLD); cr.set_font_size(11)
            cr.move_to(x+9,y+19); cr.show_text(label)
        return False

    def place(self,x,y):
        self.x=float(x); self.y=float(y)
        self.move(int(self.x)-13,int(self.y)-8)

    def schedule_hide(self,delay=1800):
        if self.hide_source:
            try: GLib.source_remove(self.hide_source)
            except Exception: pass
            self.hide_source=None
        def auto_hide():
            self.hide_source=None
            self.hide()
            return False
        self.hide_source=GLib.timeout_add(delay,auto_hide)

    def animate(self,x,y,label,state):
        self.label=label or 'Fatir'; self.state=state or 'control'; self.tx=float(x); self.ty=float(y)
        if self.hide_source:
            try: GLib.source_remove(self.hide_source)
            except Exception: pass
            self.hide_source=None
        self.show_all(); self.queue_draw()
        sx,sy=self.x,self.y; dx=self.tx-sx; dy=self.ty-sy
        steps=max(8,min(24,int((abs(dx)+abs(dy))/45)+8)); frame={'i':0}
        if self.anim_source:
            try: GLib.source_remove(self.anim_source)
            except Exception: pass
            self.anim_source=None
        def tick():
            frame['i']+=1; t=min(1.0,frame['i']/steps); e=1-(1-t)**3
            self.place(sx+dx*e,sy+dy*e); self.queue_draw()
            if t>=1:
                self.anim_source=None
                self.schedule_hide()
                return False
            return True
        self.anim_source=GLib.timeout_add(18,tick)

    def handle(self,data):
        if not data.get('visible',True):
            if self.hide_source:
                try: GLib.source_remove(self.hide_source)
                except Exception: pass
                self.hide_source=None
            self.hide(); return False
        self.animate(data.get('x',self.x),data.get('y',self.y),data.get('label','Fatir'),data.get('state','control'))
        return False

pointer=Pointer()

def server():
    try: os.unlink(sock_path)
    except FileNotFoundError: pass
    os.makedirs(os.path.dirname(sock_path),exist_ok=True)
    s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); s.bind(sock_path); s.listen(8)
    os.chmod(sock_path,0o600)
    while True:
        if parent and not os.path.exists('/proc/%d'%parent):
            GLib.idle_add(Gtk.main_quit); break
        try:
            s.settimeout(1.0); c,_=s.accept()
        except socket.timeout: continue
        try:
            payload=b''
            while True:
                chunk=c.recv(65536)
                if not chunk: break
                payload+=chunk
            if payload:
                data=json.loads(payload.decode('utf-8')); GLib.idle_add(pointer.handle,data)
        except Exception: pass
        finally:
            try:c.close()
            except Exception:pass
    try:s.close()
    except Exception:pass
    try:os.unlink(sock_path)
    except Exception:pass

threading.Thread(target=server,daemon=True).start()
Gtk.main()
"#;

fn ensure_service() -> Result<()> {
    let path = socket_path();
    if UnixStream::connect(&path).is_ok() { return Ok(()); }
    if path.exists() { let _ = fs::remove_file(&path); }
    if let Some(parent)=path.parent(){fs::create_dir_all(parent)?;}
    Command::new("python3")
        .arg("-c").arg(OVERLAY_PY)
        .arg(path.to_string_lossy().as_ref())
        .arg(std::process::id().to_string())
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().context("Could not start Fatir virtual pointer overlay")?;
    for _ in 0..20 {
        if UnixStream::connect(&path).is_ok() { return Ok(()); }
        thread::sleep(Duration::from_millis(50));
    }
    Err(anyhow!("Fatir virtual pointer overlay did not start. Ensure python3-gi, python3-cairo and GTK 3 are installed."))
}

fn send(value: serde_json::Value) -> Result<()> {
    ensure_service()?;
    let path=socket_path();
    let mut stream=UnixStream::connect(&path).context("Could not connect to Fatir virtual pointer overlay")?;
    stream.write_all(value.to_string().as_bytes())?;
    Ok(())
}

fn send_if_running(value: serde_json::Value) -> Result<()> {
    let path=socket_path();
    if !path.exists() { return Ok(()); }
    match UnixStream::connect(&path) {
        Ok(mut stream) => { stream.write_all(value.to_string().as_bytes())?; Ok(()) }
        Err(_) => { let _ = fs::remove_file(&path); Ok(()) }
    }
}

pub fn enter_browser() -> Result<()> {
    ensure_enabled()?;
    SURFACE.store(2, Ordering::Relaxed);
    // The browser renders its own in-page Fatir pointer. Hide the desktop overlay
    // before browser control starts so two Fatir pointers can never be visible.
    let _ = send_if_running(json!({"visible":false}));
    Ok(())
}

pub fn enter_desktop() -> Result<()> {
    ensure_enabled()?;
    SURFACE.store(1, Ordering::Relaxed);
    Ok(())
}

pub fn move_to(x: i64, y: i64, label: &str, state: &str) -> Result<()> {
    enter_desktop()?;
    send(json!({"visible":true,"x":x,"y":y,"label":label,"state":state}))
}

pub fn hide() -> Result<()> {
    SURFACE.store(0, Ordering::Relaxed);
    send_if_running(json!({"visible":false}))
}

pub fn move_to_bounds(bounds: &serde_json::Value, label: &str, state: &str) -> Result<()> {
    let x=bounds.get("x").and_then(serde_json::Value::as_i64).ok_or_else(||anyhow!("Target has no screen x coordinate"))?;
    let y=bounds.get("y").and_then(serde_json::Value::as_i64).ok_or_else(||anyhow!("Target has no screen y coordinate"))?;
    let w=bounds.get("width").and_then(serde_json::Value::as_i64).unwrap_or(1).max(1);
    let h=bounds.get("height").and_then(serde_json::Value::as_i64).unwrap_or(1).max(1);
    move_to(x+w/2,y+h/2,label,state)
}
