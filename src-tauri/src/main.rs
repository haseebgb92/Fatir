use fatir::{
    adaptive, app_playbooks, auth_watcher, browser_memory, cleanup, continuations, credentials,
    desktop, health, jobs, memory, ollama, orchestrator, permissions, pointer, proactive, projects,
    recovery, rollback, routines, schedules, share, software, tasks, teach, terminal_sessions, tools, observer,
};
use fatir::models::{AgentResponse, Attachment, SystemSnapshot};
use fatir::ollama::SharedState;
use fatir::observer::ObserverConfig;
use serde::Serialize;
use std::{fs, path::PathBuf, process::{Command as StdCommand, Stdio}, sync::{Mutex, atomic::{AtomicBool, AtomicU32, Ordering}}};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use tauri::{LogicalSize, Manager, PhysicalPosition, Position, Size, WindowEvent, WebviewUrl, WebviewWindowBuilder};



#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn malloc_trim(pad: usize) -> i32;
}

fn schedule_heap_trim() {
    #[cfg(target_os = "linux")]
    tauri::async_runtime::spawn(async {
        // Give WebKit/GTK a moment to release the destroyed webview's objects,
        // then ask glibc to return free heap pages to the OS.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        unsafe { let _ = malloc_trim(0); }
    });
}

struct PanelUiState {
    pinned: AtomicBool,
    width: AtomicU32,
    side: Mutex<String>,
    mode: Mutex<String>,
}

impl Default for PanelUiState {
    fn default() -> Self {
        Self {
            pinned: AtomicBool::new(false),
            width: AtomicU32::new(420),
            side: Mutex::new("right".into()),
            mode: Mutex::new("open".into()),
        }
    }
}

#[derive(Default)]
struct ShareUiState {
    pending_paths: Mutex<Vec<String>>,
}

fn extract_share_args(args: &[String]) -> Vec<String> {
    let Some(pos) = args.iter().position(|x| x == "--share") else { return Vec::new(); };
    args.iter().skip(pos + 1).filter(|x| !x.trim().is_empty()).cloned().collect()
}

fn set_pending_share(app: &tauri::AppHandle, args: &[String]) {
    let paths = extract_share_args(args);
    if paths.is_empty() { return; }
    let state = app.state::<ShareUiState>();
    let mut pending = match state.pending_paths.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *pending = paths;
}

#[derive(Serialize)]
struct PanelState {
    pinned: bool,
    width: u32,
    side: String,
    mode: String,
}

fn panel_snapshot(state: &PanelUiState) -> PanelState {
    let side = state.side.lock().map(|v| v.clone()).unwrap_or_else(|_| "right".into());
    let mode = state.mode.lock().map(|v| v.clone()).unwrap_or_else(|_| "open".into());
    PanelState {
        pinned: state.pinned.load(Ordering::Relaxed),
        width: state.width.load(Ordering::Relaxed),
        side,
        mode,
    }
}

#[derive(Serialize)]
struct AppStatus {
    connected: bool,
    connection: String,
    has_cloud_key: bool,
    version: String,
}

#[tauri::command]
async fn app_status() -> Result<AppStatus, String> {
    match ollama::test_connection().await {
        Ok(message) => Ok(AppStatus { connected: true, connection: message, has_cloud_key: ollama::api_key().is_some(), version: env!("CARGO_PKG_VERSION").into() }),
        Err(err) => Ok(AppStatus { connected: false, connection: err.to_string(), has_cloud_key: ollama::api_key().is_some(), version: env!("CARGO_PKG_VERSION").into() }),
    }
}


#[tauri::command]
async fn list_models() -> Result<Vec<serde_json::Value>, String> {
    ollama::list_models().await.map_err(|e| e.to_string())
}

#[tauri::command]
fn save_api_key(key: String) -> Result<(), String> {
    ollama::save_api_key(&key).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_api_key() -> Result<(), String> {
    ollama::delete_api_key().map_err(|e| e.to_string())
}

#[tauri::command]
async fn pending_actions(state: tauri::State<'_, SharedState>) -> Result<Vec<fatir::models::PendingAction>, String> {
    let shared = state.inner().clone();
    ollama::refresh_pending_from_disk(&shared).await;
    Ok(ollama::pending_actions(&shared).await)
}

#[tauri::command]
async fn cancel_run(
    state: tauri::State<'_, SharedState>,
    session_id: String,
) -> Result<bool, String> {
    Ok(ollama::cancel_run(state.inner().clone(), &session_id).await)
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Only http/https links can be opened".into());
    }
    std::process::Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_local_path(path: String) -> Result<(), String> {
    let target = if path.starts_with("~/") {
        dirs::home_dir().unwrap_or_default().join(path.trim_start_matches("~/")).display().to_string()
    } else if path == "~" {
        dirs::home_dir().unwrap_or_default().display().to_string()
    } else {
        path
    };
    std::process::Command::new("xdg-open")
        .arg(&target)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn send_message(
    state: tauri::State<'_, SharedState>,
    session_id: String,
    text: String,
    attachments: Vec<Attachment>,
    mode: String,
) -> Result<AgentResponse, String> {
    ollama::send_message(state.inner().clone(), &session_id, &text, attachments, &mode).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn approve_action(
    state: tauri::State<'_, SharedState>,
    action_id: String,
    mode: String,
) -> Result<AgentResponse, String> {
    ollama::approve_action(state.inner().clone(), &action_id, &mode).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn deny_action(
    state: tauri::State<'_, SharedState>,
    action_id: String,
    mode: String,
) -> Result<AgentResponse, String> {
    ollama::deny_action(state.inner().clone(), &action_id, &mode).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn pick_files() -> Vec<String> {
    rfd::FileDialog::new().pick_files().unwrap_or_default().into_iter().map(|p| p.display().to_string()).collect()
}

#[tauri::command]
async fn capture_screenshot() -> Result<String, String> {
    let args = serde_json::json!({});
    tools::execute("take_screenshot", &args, ollama::api_key().as_deref()).await
        .map(|(result, _)| result)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn system_snapshot() -> Result<SystemSnapshot, String> {
    tools::system_snapshot().await.map_err(|e| e.to_string())
}

#[tauri::command]
fn observer_status() -> serde_json::Value { observer::status() }

#[tauri::command]
fn set_observer_config(config: ObserverConfig) -> Result<(), String> {
    observer::save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_observer_data() -> Result<(), String> {
    memory::clear_observations().map_err(|e| e.to_string())
}

#[tauri::command]
fn recent_actions(limit: usize) -> Result<Vec<serde_json::Value>, String> {
    memory::recent_actions(limit.clamp(1, 200)).map_err(|e| e.to_string())
}

#[tauri::command]
fn routine_summary(limit: usize) -> Result<serde_json::Value, String> {
    memory::routine_summary(limit.clamp(1, 5000)).map_err(|e| e.to_string())
}

#[tauri::command]
fn panel_state(state: tauri::State<'_, PanelUiState>) -> PanelState {
    panel_snapshot(state.inner())
}

#[tauri::command]
fn set_panel_pinned(state: tauri::State<'_, PanelUiState>, pinned: bool) -> PanelState {
    state.pinned.store(pinned, Ordering::Relaxed);
    panel_snapshot(state.inner())
}

#[tauri::command]
fn set_panel_layout(
    app: tauri::AppHandle,
    state: tauri::State<'_, PanelUiState>,
    width: u32,
    side: String,
    mode: String,
) -> Result<PanelState, String> {
    let width = width.clamp(360, 520);
    let side = if side.eq_ignore_ascii_case("left") { "left" } else { "right" };
    let mode = if mode.eq_ignore_ascii_case("peek") { "peek" } else { "open" };
    state.width.store(width, Ordering::Relaxed);
    if let Ok(mut value) = state.side.lock() { *value = side.into(); }
    if let Ok(mut value) = state.mode.lock() { *value = mode.into(); }
    if let Some(window) = app.get_webview_window("main") {
        apply_panel_layout(&app, &window);
    }
    Ok(panel_snapshot(state.inner()))
}

#[tauri::command]
fn hide_panel(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.destroy().map_err(|e| e.to_string())?;
        schedule_heap_trim();
    }
    Ok(())
}


#[tauri::command]
fn task_list(include_completed: bool) -> Result<Vec<serde_json::Value>, String> {
    tasks::list(include_completed).map_err(|e| e.to_string())
}

#[tauri::command]
fn job_list() -> Result<Vec<serde_json::Value>, String> {
    jobs::list().map_err(|e| e.to_string())
}

#[tauri::command]
fn job_log(job_id: String, lines: usize) -> Result<String, String> {
    jobs::log_tail(&job_id, lines.clamp(1, 500)).map_err(|e| e.to_string())
}

#[tauri::command]
fn job_cancel(job_id: String) -> Result<serde_json::Value, String> {
    jobs::cancel(&job_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn rollback_list(limit: usize) -> Result<Vec<serde_json::Value>, String> {
    rollback::list(limit.clamp(1, 200)).map_err(|e| e.to_string())
}

#[tauri::command]
fn rollback_execute(id: String) -> Result<serde_json::Value, String> {
    rollback::execute(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn credential_list() -> Result<Vec<serde_json::Value>, String> {
    credentials::list().map_err(|e| e.to_string())
}

#[tauri::command]
fn credential_store(label: String, account: String, secret: String) -> Result<serde_json::Value, String> {
    credentials::store(&label, &account, &secret).map_err(|e| e.to_string())
}

#[tauri::command]
fn credential_remove(id: String) -> Result<(), String> {
    credentials::remove(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn sudo_credential_status() -> serde_json::Value {
    serde_json::json!({"stored": credentials::has_sudo()})
}

#[tauri::command]
fn sudo_credential_store(secret: String) -> Result<serde_json::Value, String> {
    credentials::store_sudo(&secret).map_err(|e| e.to_string())
}

#[tauri::command]
fn sudo_credential_remove() -> Result<(), String> {
    credentials::remove_sudo().map_err(|e| e.to_string())
}


#[tauri::command]
async fn health_report() -> Result<serde_json::Value, String> {
    health::report().await.map_err(|e| e.to_string())
}


#[tauri::command]
fn cleanup_scan(path: Option<String>, min_age_days: Option<u64>, limit: Option<usize>) -> Result<serde_json::Value, String> {
    cleanup::scan(path.as_deref(), min_age_days.unwrap_or(14), limit.unwrap_or(120)).map_err(|e| e.to_string())
}

#[tauri::command]
fn cleanup_duplicates(path: Option<String>, minimum_size_mb: Option<u64>, limit_groups: Option<usize>) -> Result<serde_json::Value, String> {
    cleanup::duplicate_scan(path.as_deref(), minimum_size_mb.unwrap_or(5), limit_groups.unwrap_or(40)).map_err(|e| e.to_string())
}

#[tauri::command]
fn trash_status(limit: Option<usize>) -> Result<serde_json::Value, String> {
    cleanup::trash_summary(limit.unwrap_or(80)).map_err(|e| e.to_string())
}

#[tauri::command]
fn routine_list() -> Result<Vec<serde_json::Value>, String> {
    routines::list().map_err(|e| e.to_string())
}

#[tauri::command]
fn routine_remove(id: String) -> Result<serde_json::Value, String> {
    routines::remove(&id).map_err(|e| e.to_string())
}



#[tauri::command]
fn permission_status() -> serde_json::Value { permissions::status() }

#[tauri::command]
fn set_permission_config(config: permissions::PermissionConfig) -> Result<(), String> { permissions::save(&config).map_err(|e|e.to_string()) }

#[tauri::command]
fn project_list() -> Result<Vec<serde_json::Value>, String> { projects::list().map_err(|e|e.to_string()) }

#[tauri::command]
fn terminal_session_list() -> Result<Vec<serde_json::Value>, String> { terminal_sessions::list().map_err(|e|e.to_string()) }

#[tauri::command]
fn teach_mode_status() -> serde_json::Value { teach::status() }

#[tauri::command]
fn v1_status() -> Result<serde_json::Value, String> {
    let tasks=tasks::list(false).map_err(|e|e.to_string())?;
    let jobs=jobs::list().map_err(|e|e.to_string())?;
    let projects=projects::list().map_err(|e|e.to_string())?;
    let terminals=terminal_sessions::list().map_err(|e|e.to_string())?;
    let schedules=schedules::list().map_err(|e|e.to_string())?;
    let failures=memory::failure_summary(300).map_err(|e|e.to_string())?;
    Ok(serde_json::json!({
        "version":env!("CARGO_PKG_VERSION"),"engine":orchestrator::status(),
        "active_tasks":tasks.len(),"running_jobs":jobs.iter().filter(|j|j.get("status").and_then(serde_json::Value::as_str)==Some("running")).count(),
        "projects":projects.len(),"terminal_sessions":terminals.len(),"schedules":schedules.len(),"teach":teach::status(),"permissions":permissions::status(),"computer_control":pointer::status(),"continuations":continuations::status(),"auth_watcher":auth_watcher::status(),"failures":failures
    }))
}

#[tauri::command]
fn proactive_events(limit: Option<usize>) -> Result<Vec<serde_json::Value>, String> {
    proactive::events(limit.unwrap_or(20)).map_err(|e| e.to_string())
}

#[tauri::command]
fn proactive_status() -> serde_json::Value { proactive::status() }

#[tauri::command]
async fn proactive_check_now() -> Result<serde_json::Value, String> {
    proactive::run_once().await.map_err(|e| e.to_string())
}


#[tauri::command]
fn proactive_ack(id: String) -> Result<serde_json::Value, String> {
    proactive::acknowledge(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_proactive_config(config: proactive::ProactiveConfig) -> Result<(), String> {
    proactive::save_config(&config).map_err(|e| e.to_string())
}


#[tauri::command]
fn adaptive_status() -> serde_json::Value { adaptive::status() }

#[tauri::command]
fn adaptive_suggestions(limit: Option<usize>) -> Vec<serde_json::Value> { adaptive::suggestions(limit.unwrap_or(8)) }

#[tauri::command]
fn set_adaptive_config(config: adaptive::AdaptiveConfig) -> Result<(), String> {
    adaptive::save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_adaptive_learning() -> Result<(), String> {
    adaptive::clear().map_err(|e| e.to_string())
}


#[tauri::command]
fn computer_control_state() -> pointer::ComputerControlState { pointer::status() }

#[tauri::command]
async fn set_computer_control_paused(paused: bool) -> pointer::ComputerControlState {
    let state = pointer::set_paused(paused);
    if paused { tools::hide_all_virtual_pointers().await; }
    state
}

#[tauri::command]
fn share_pending(state: tauri::State<'_, ShareUiState>) -> Vec<String> {
    match state.pending_paths.lock() {
        Ok(mut pending) => std::mem::take(&mut *pending),
        Err(_) => Vec::new(),
    }
}

#[tauri::command]
fn share_describe(paths: Vec<String>) -> Result<Vec<share::ShareFile>, String> {
    share::describe(&paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn share_copy(paths: Vec<String>) -> Result<serde_json::Value, String> {
    share::copy(&paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn share_copy_paths(paths: Vec<String>) -> Result<serde_json::Value, String> {
    share::copy_paths(&paths).map_err(|e| e.to_string())
}

#[tauri::command]
fn share_latest_screenshot() -> Result<share::ShareFile, String> {
    share::latest_screenshot().map_err(|e| e.to_string())
}

#[tauri::command]
async fn share_whatsapp_contacts() -> Result<serde_json::Value, String> {
    tools::with_browser_execution_allowed(true, share::whatsapp_contacts()).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn share_whatsapp_send(paths: Vec<String>, contact: String) -> Result<serde_json::Value, String> {
    tools::with_browser_execution_allowed(true, share::whatsapp_send(&paths, &contact)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn share_email_draft(paths: Vec<String>, to: String, subject: String) -> Result<serde_json::Value, String> {
    tools::with_browser_execution_allowed(true, share::email_draft(&paths, &to, &subject)).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn share_history(limit: usize) -> Vec<serde_json::Value> { share::history(limit.clamp(1, 100)) }

#[tauri::command]
fn reveal_data_dir() -> Result<(), String> {
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("xdg-open").arg(&dir).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir")
}

fn apply_panel_layout(app: &tauri::AppHandle, window: &tauri::WebviewWindow) {
    let state = app.state::<PanelUiState>();
    let configured_width = state.width.load(Ordering::Relaxed).clamp(360, 520);
    let mode = state.mode.lock().map(|v| v.clone()).unwrap_or_else(|_| "open".into());
    let side = state.side.lock().map(|v| v.clone()).unwrap_or_else(|_| "right".into());
    let logical_width = if mode == "peek" { configured_width.min(360) } else { configured_width };

    if let Ok(Some(monitor)) = window.current_monitor() {
        let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
        let m = monitor.size();
        let monitor_height_logical = m.height as f64 / scale;
        let logical_height = if mode == "peek" {
            520.0_f64.min((monitor_height_logical - 72.0).max(420.0))
        } else {
            (monitor_height_logical - 58.0).max(620.0)
        };
        let _ = window.set_size(Size::Logical(LogicalSize::new(logical_width as f64, logical_height)));

        let physical_width = (logical_width as f64 * scale).round().max(1.0) as u32;
        let margin = (14.0 * scale).round() as u32;
        let x = if side == "left" {
            margin as i32
        } else {
            m.width.saturating_sub(physical_width + margin) as i32
        };
        let y = (34.0 * scale).round() as i32;
        let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    }
}

fn ensure_main_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(window) = app.get_webview_window("main") {
        return Some(window);
    }
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Fatir")
        .inner_size(420.0, 780.0)
        .min_inner_size(360.0, 520.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .visible(false)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()
        .ok()
}

fn show_panel(app: &tauri::AppHandle) {
    if let Some(window) = ensure_main_window(app) {
        apply_panel_layout(app, &window);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn ensure_core_running() {
    #[cfg(unix)]
    {
        let socket = data_dir().join("core.sock");
        if UnixStream::connect(&socket).is_ok() {
            return;
        }
    }

    if let Ok(mut child) = StdCommand::new("fatir-core")
        .arg("--background")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn()
    {
        // Reap the helper if it exits (for example, a race where another core
        // wins startup). This prevents defunct fatir-core children under the UI.
        std::thread::spawn(move || { let _ = child.wait(); });
    }
}

fn main() {
    let startup_args: Vec<String> = std::env::args().collect();
    let background = startup_args.iter().any(|a| a == "--background");
    if background {
        ensure_core_running();
        return;
    }
    ensure_core_running();
    let state = ollama::new_state();
    let startup_share = extract_share_args(&startup_args);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            set_pending_share(app, &args);
            show_panel(app);
        }))
        .manage(state)
        .manage(PanelUiState::default())
        .manage(ShareUiState { pending_paths: Mutex::new(startup_share.clone()) })
        .invoke_handler(tauri::generate_handler![
            app_status,
            list_models,
            save_api_key,
            clear_api_key,
            send_message,
            cancel_run,
            pending_actions,
            open_local_path,
            open_external_url,
            approve_action,
            deny_action,
            pick_files,
            capture_screenshot,
            share_pending,
            share_describe,
            share_copy,
            share_copy_paths,
            share_latest_screenshot,
            share_whatsapp_contacts,
            share_whatsapp_send,
            share_email_draft,
            share_history,
            system_snapshot,
            recent_actions,
            routine_summary,
            panel_state,
            set_panel_pinned,
            set_panel_layout,
            hide_panel,
            reveal_data_dir,
            observer_status,
            set_observer_config,
            clear_observer_data,
            task_list,
            job_list,
            job_log,
            job_cancel,
            rollback_list,
            rollback_execute,
            credential_list,
            credential_store,
            credential_remove,
            sudo_credential_status,
            sudo_credential_store,
            sudo_credential_remove,
            cleanup_scan,
            cleanup_duplicates,
            trash_status,
            routine_list,
            routine_remove,
            project_list,
            terminal_session_list,
            teach_mode_status,
            v1_status,
            permission_status,
            set_permission_config,
            proactive_events,
            proactive_status,
            proactive_check_now,
            proactive_ack,
            set_proactive_config,
            adaptive_status,
            adaptive_suggestions,
            set_adaptive_config,
            clear_adaptive_learning,
            computer_control_state,
            set_computer_control_paused,
            health_report
        ])
        .setup(move |app| {
            fs::create_dir_all(data_dir()).ok();
            adaptive::ensure_layout().ok();
            show_panel(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                WindowEvent::CloseRequested { .. } => {
                    let _ = window.destroy();
                }
                WindowEvent::Focused(false) => {
                    // Keep the panel open on focus changes. CloseRequested and the
                    // explicit hide action are the only hide paths. This avoids the
                    // single-instance launcher callback showing Fatir and an immediate
                    // focus-loss event hiding it again.
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Fatir");

    app.run(|_, _| {});
}