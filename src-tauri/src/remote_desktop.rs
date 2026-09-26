use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::{
    env,
    net::{Ipv4Addr, SocketAddrV4, TcpListener as StdTcpListener},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::TcpStream,
    process::{Child, Command},
    sync::Mutex,
    time::sleep,
};

#[derive(Debug, Clone, Serialize)]
pub struct RemoteDesktopStatus {
    pub available: bool,
    pub running: bool,
    pub backend: &'static str,
    pub display: String,
    pub message: String,
}

#[derive(Default)]
struct RemoteState {
    child: Option<Child>,
    port: Option<u16>,
}

#[derive(Clone, Default)]
pub struct RemoteDesktopService {
    inner: Arc<Mutex<RemoteState>>,
}

impl RemoteDesktopService {
    pub fn new() -> Self { Self::default() }

    pub async fn status(&self) -> RemoteDesktopStatus {
        let available = command_exists("x11vnc");
        let display = display_name();
        let running = {
            let mut state = self.inner.lock().await;
            child_running(&mut state).await
        };
        let message = if !available {
            "x11vnc is not installed. Re-run the Fatir installer to add Remote Desktop support.".into()
        } else if !is_x11_session() {
            "This Remote Desktop Core build currently requires an X11 session.".into()
        } else if running {
            "Remote desktop is ready.".into()
        } else {
            "Remote desktop is available.".into()
        };
        RemoteDesktopStatus { available: available && is_x11_session(), running, backend: "x11vnc", display, message }
    }

    pub async fn ensure_started(&self) -> Result<u16> {
        if !command_exists("x11vnc") {
            return Err(anyhow!("x11vnc is not installed; re-run ./install.sh"));
        }
        if !is_x11_session() {
            return Err(anyhow!("Remote Desktop Core v0.5 currently supports X11 sessions; Wayland/PipeWire support comes next"));
        }

        {
            let mut state = self.inner.lock().await;
            if child_running(&mut state).await {
                if let Some(port) = state.port { return Ok(port); }
            }
        }

        let port = reserve_loopback_port()?;
        let display = display_name();
        let mut command = Command::new("x11vnc");
        command
            .arg("-display").arg(&display)
            .arg("-localhost")
            .arg("-nopw")
            .arg("-forever")
            .arg("-shared")
            // The local monitor is 2560×1440 on the primary Fatir machine.
            // Half-scale keeps the first raw RFB frame practical over Tailscale
            // while x11vnc maps input back to the real desktop coordinates.
            .arg("-scale").arg("0.5")
            .arg("-rfbport").arg(port.to_string())
            .arg("-quiet")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        if let Ok(authority) = env::var("XAUTHORITY") {
            if !authority.trim().is_empty() {
                command.arg("-auth").arg(authority);
            }
        }

        let mut child = command.spawn().context("Could not start x11vnc")?;

        let mut ready = false;
        for _ in 0..30 {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(anyhow!("x11vnc exited before becoming ready: {status}"));
            }
            if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                ready = true;
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        if !ready {
            let _ = child.kill().await;
            return Err(anyhow!("x11vnc did not become ready"));
        }

        let mut state = self.inner.lock().await;
        state.child = Some(child);
        state.port = Some(port);
        Ok(port)
    }

    pub async fn stop(&self) -> Result<()> {
        let mut state = self.inner.lock().await;
        if let Some(child) = state.child.as_mut() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        state.child = None;
        state.port = None;
        Ok(())
    }
}

async fn child_running(state: &mut RemoteState) -> bool {
    let Some(child) = state.child.as_mut() else { return false; };
    match child.try_wait() {
        Ok(None) => true,
        _ => {
            state.child = None;
            state.port = None;
            false
        }
    }
}

fn display_name() -> String {
    env::var("DISPLAY").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| ":0".into())
}

fn is_x11_session() -> bool {
    env::var("XDG_SESSION_TYPE")
        .map(|v| !v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(true)
}

fn command_exists(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else { return false; };
    env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

fn reserve_loopback_port() -> Result<u16> {
    let listener = StdTcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    Ok(listener.local_addr()?.port())
}
