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
    process::{Child, ChildStdout, Command},
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
        let available = command_exists("x11vnc") && command_exists("ffmpeg");
        let display = display_name();
        let running = {
            let mut state = self.inner.lock().await;
            child_running(&mut state).await
        };
        let message = if !available {
            "Remote Desktop requires x11vnc and FFmpeg. Re-run the Fatir installer to add them.".into()
        } else if !is_x11_session() {
            "This Remote Desktop Core build currently requires an X11 session.".into()
        } else if running {
            "Remote desktop is ready.".into()
        } else {
            "Remote desktop is available.".into()
        };
        RemoteDesktopStatus { available: available && is_x11_session(), running, backend: "h264+x11vnc-control", display, message }
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
            // Keep the transport light, but poll/send much more aggressively
            // than x11vnc's 20ms + 20ms defaults. CopyRect/Hextile on the client
            // then avoid paying raw-pixel bandwidth for most desktop activity.
            .arg("-scale").arg("0.5")
            .arg("-wait").arg("8")
            .arg("-defer").arg("5")
            .arg("-extra_fbur").arg("2")
            .arg("-nonap")
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
        drop(state);
        notify("Companion remote session active");
        Ok(port)
    }

    pub async fn spawn_video_stream(&self) -> Result<ChildStdout> {
        if !command_exists("ffmpeg") {
            return Err(anyhow!("ffmpeg is not installed; re-run ./install.sh"));
        }
        if !is_x11_session() {
            return Err(anyhow!("H.264 Remote Desktop currently requires an X11 session"));
        }

        // Keep x11vnc alive as the lightweight mouse/keyboard control channel.
        let _ = self.ensure_started().await?;

        let display = display_name();
        let mut command = Command::new("ffmpeg");
        command
            .arg("-nostdin")
            .arg("-hide_banner")
            .arg("-loglevel").arg("error")
            .arg("-fflags").arg("nobuffer")
            .arg("-f").arg("x11grab")
            .arg("-framerate").arg("30")
            .arg("-draw_mouse").arg("1")
            .arg("-i").arg(&display)
            .arg("-vf").arg("scale=1280:-2:flags=fast_bilinear,format=yuv420p")
            .arg("-an")
            .arg("-c:v").arg("libx264")
            .arg("-preset").arg("ultrafast")
            .arg("-tune").arg("zerolatency")
            .arg("-profile:v").arg("baseline")
            .arg("-level").arg("3.1")
            .arg("-g").arg("30")
            .arg("-keyint_min").arg("30")
            .arg("-sc_threshold").arg("0")
            .arg("-bf").arg("0")
            .arg("-refs").arg("1")
            .arg("-b:v").arg("2500k")
            .arg("-maxrate").arg("3000k")
            .arg("-bufsize").arg("500k")
            .arg("-flush_packets").arg("1")
            .arg("-muxdelay").arg("0")
            .arg("-muxpreload").arg("0")
            .arg("-f").arg("mpegts")
            .arg("pipe:1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = command.spawn().context("Could not start FFmpeg Remote Desktop stream")?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("FFmpeg did not expose a video stream"))?;

        // Reap the encoder when the HTTP client disconnects and the stdout pipe closes.
        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        Ok(stdout)
    }

    pub async fn stop(&self) -> Result<()> {
        let mut state = self.inner.lock().await;
        if let Some(child) = state.child.as_mut() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        state.child = None;
        state.port = None;
        drop(state);
        notify("Companion remote session ended");
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

fn notify(message: &str) {
    let _ = std::process::Command::new("notify-send")
        .arg("Fatir Remote Desktop")
        .arg(message)
        .spawn();
}

fn reserve_loopback_port() -> Result<u16> {
    let listener = StdTcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    Ok(listener.local_addr()?.port())
}
