use crate::{adaptive, agent_schedules, browser_memory, cleanup, credentials, desktop, jobs, memory, pointer, rollback, routines, share, software, tasks, teach, proactive, projects, terminal_sessions, orchestrator, app_playbooks, schedules, models::{LargestFile, ResourceRef, SystemSnapshot, TraceItem}};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use regex::Regex;
use reqwest::Client;
use serde_json::{json, Value};
use std::{cmp::Reverse, collections::HashSet, fs, path::{Path, PathBuf}, process::Stdio, sync::atomic::{AtomicBool, Ordering}};
use sysinfo::System;
use tokio::process::Command;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tokio::net::TcpStream;
use walkdir::WalkDir;

pub fn tool_definitions() -> Vec<Value> {
    vec![
        tool("system_snapshot", "Inspect CPU, memory, OS and root disk usage. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("health_report", "Run a focused health check for disk pressure, memory pressure and top CPU processes. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("list_directory", "List files and folders in a directory. Read-only.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})),
        tool("list_largest_files", "Find the largest files under a folder. Never deletes anything.", json!({"type":"object","properties":{"path":{"type":"string"},"minimum_size_mb":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1,"maximum":200}},"required":["path"],"additionalProperties":false})),
        tool("list_largest_directories", "Find the largest immediate directories under a folder, including hidden folders. Read-only; never deletes anything.", json!({"type":"object","properties":{"path":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}},"required":["path"],"additionalProperties":false})),
        tool("cleanup_scan", "Analyze Downloads or another folder for high-confidence cleanup candidates and uncertain review items. Read-only; never deletes anything and never labels personal files safe just because they are old or large.", json!({"type":"object","properties":{"path":{"type":"string"},"min_age_days":{"type":"integer","minimum":1,"maximum":3650},"limit":{"type":"integer","minimum":1,"maximum":500}},"additionalProperties":false})),
        tool("duplicate_scan", "Find byte-for-byte duplicate files using size grouping and SHA-256. Read-only. Returns duplicate groups and reclaimable size; keep at least one copy.", json!({"type":"object","properties":{"path":{"type":"string"},"minimum_size_mb":{"type":"integer","minimum":1,"maximum":10240},"limit_groups":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false})),
        tool("trash_inventory", "Inspect desktop Trash, total size and largest trashed files. Read-only.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":200}},"additionalProperties":false})),
        tool("cleanup_move_to_trash", "Move multiple explicitly selected files/folders to desktop Trash. Requires approval. Never permanently erases them.", json!({"type":"object","properties":{"paths":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":100}},"required":["paths"],"additionalProperties":false})),
        tool("empty_trash", "Permanently empty desktop Trash. Irreversible and always requires explicit approval.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("read_file", "Read and extract useful content from a local text, code, PDF or DOCX file. Read-only. For PDFs this extracts text; use render_pdf_pages when visual page content matters.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})),
        tool("render_pdf_pages", "Render a page range from a local PDF into images so you can visually inspect scans, diagrams, charts, screenshots, photographs, layout and other content that text extraction cannot see. Read-only. Inspect at most 12 pages per call.", json!({"type":"object","properties":{"path":{"type":"string"},"start_page":{"type":"integer","minimum":1},"end_page":{"type":"integer","minimum":1}},"required":["path","start_page","end_page"],"additionalProperties":false})),
        tool("open_path", "Open a LOCAL file or folder in the user's normal desktop application. Do not use this for websites; use browser_open_url for http/https.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})),
        tool("write_text_file", "Create or replace a local text file with automatic rollback checkpoint. Requires approval.", json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"create_parents":{"type":"boolean"}},"required":["path","content"],"additionalProperties":false})),
        tool("copy_file", "Copy a local file to a new destination with rollback protection for the destination. Requires approval.", json!({"type":"object","properties":{"source":{"type":"string"},"destination":{"type":"string"}},"required":["source","destination"],"additionalProperties":false})),
        tool("move_path", "Move/rename a local file or folder to a destination that does not already exist. Records an undo move. Requires approval.", json!({"type":"object","properties":{"source":{"type":"string"},"destination":{"type":"string"}},"required":["source","destination"],"additionalProperties":false})),
        tool("create_directory", "Create a local directory and record an undo point when it is newly created. Requires approval.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})),
        tool("take_screenshot", "Capture the current desktop to an image and return its path.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("browser_observe", "Capture the current Fatir-controlled browser webpage without moving the user's physical mouse. Fatir has its own visible virtual pointer in the controlled browser. Returns a webpage screenshot for the vision model. Use browser_elements before guessing coordinates.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("browser_elements", "Inspect the current webpage through Chrome DevTools MCP accessibility snapshot. Returns semantic roles, labels and MCP uids for buttons, links, inputs and controls. Prefer targeted tools such as browser_click_text when the requested control name is already known.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("browser_click_text", "Find a visible button/link/control by its human-readable text and activate it through Chrome DevTools MCP semantic targeting. Prefer this when the user names the control, for example Continue with Google. The physical mouse is never moved.", json!({"type":"object","properties":{"text":{"type":"string"},"exact":{"type":"boolean"}},"required":["text"],"additionalProperties":false})),
        tool("browser_fill_by_label", "Find a visible webpage input by label, placeholder, name or nearby text and fill it in one browser operation. For ordinary non-sensitive text only; use browser_fill_credential for stored secrets.", json!({"type":"object","properties":{"label":{"type":"string"},"text":{"type":"string"}},"required":["label","text"],"additionalProperties":false})),
        tool("browser_upload_file", "Atomically attach a LOCAL file to the current webpage. Generic across websites: Fatir rescans dynamic DOM, can reveal attachment/upload controls, intercepts chooser-backed file inputs, and keeps browser handles in one DevTools session so reactive page rerenders do not break the upload. Prefer this over manually clicking attachment menus. An explicit user request to upload/share the file is sufficient authorization; do not ask again.", json!({"type":"object","properties":{"path":{"type":"string"},"hint":{"type":"string","description":"Optional clue such as document, image, video, attachment, profile photo, or a nearby label."}},"required":["path"],"additionalProperties":false})),
        tool("share_whatsapp_send", "Send one or more LOCAL files to a named WhatsApp contact using Fatir Share and the controlled browser. This is a generic local-file handoff path. When the user explicitly asked to send/share these files, execute without a second approval prompt.", json!({"type":"object","properties":{"paths":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":25},"contact":{"type":"string"}},"required":["paths","contact"],"additionalProperties":false})),
        tool("share_email_draft", "Create a Gmail draft to an email address and attach one or more LOCAL files using Fatir's controlled browser. It does not press Send. An explicit user request to prepare/share the files is sufficient authorization; do not ask again.", json!({"type":"object","properties":{"paths":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":25},"to":{"type":"string"},"subject":{"type":"string"}},"required":["paths","to"],"additionalProperties":false})),
        tool("share_latest_screenshot", "Find the newest screenshot in ~/Pictures/screenshot (or Fatir's screenshot folder). Read-only and returns the file path.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("browser_page_summary", "Read a compact DOM summary of the current webpage: URL, title, headings, visible controls and a bounded amount of visible text. Prefer this over screenshots when visual interpretation is unnecessary.", json!({"type":"object","properties":{"max_chars":{"type":"integer","minimum":1000,"maximum":20000}},"additionalProperties":false})),
        tool("amazon_keyword_research", "High-efficiency Amazon keyword research skill. Searches Amazon.com for a keyword, collects the first organic product results, visits each product page inside Fatir's controlled browser, extracts title, ASIN, current displayed price and feature bullets, and writes a CSV. Use this instead of manually opening/clicking five products. Read-only web research plus local CSV creation.", json!({"type":"object","properties":{"keyword":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":10},"output_path":{"type":"string"}},"required":["keyword"],"additionalProperties":false})),
        tool("browser_click_element", "Activate a webpage element by its MCP uid from browser_elements (legacy Fatir IDs remain supported as fallback). This uses browser automation and never moves the user's physical mouse pointer.", json!({"type":"object","properties":{"element_id":{"type":"string"}},"required":["element_id"],"additionalProperties":false})),
        tool("browser_fill_element", "Fill a webpage input by its MCP uid from browser_elements (legacy Fatir IDs remain supported as fallback). Use only for ordinary non-sensitive text. Never type passwords, one-time codes, recovery codes, payment data or API keys.", json!({"type":"object","properties":{"element_id":{"type":"string"},"text":{"type":"string"}},"required":["element_id","text"],"additionalProperties":false})),
        tool("browser_click", "Fallback webpage click at screenshot-relative coordinates. This is sent through Chrome DevTools and does NOT move the user's physical mouse. Prefer browser_elements + browser_click_element whenever possible.", json!({"type":"object","properties":{"x":{"type":"integer","minimum":0},"y":{"type":"integer","minimum":0}},"required":["x","y"],"additionalProperties":false})),
        tool("browser_type", "Insert ordinary non-sensitive text into the focused webpage field without using the user's keyboard or mouse. Never type passwords, one-time codes, recovery codes, payment data or API keys.", json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false})),
        tool("browser_key", "Send a webpage key such as Return, Tab, Escape, BackSpace, ArrowDown or ctrl+a through browser automation. It does not take over the physical keyboard.", json!({"type":"object","properties":{"key":{"type":"string"}},"required":["key"],"additionalProperties":false})),
        tool("browser_scroll", "Scroll the Fatir-controlled webpage without moving the physical mouse.", json!({"type":"object","properties":{"direction":{"type":"string","enum":["up","down"]},"steps":{"type":"integer","minimum":1,"maximum":20}},"required":["direction"],"additionalProperties":false})),
        tool("browser_open_url", "Open an http or https URL in Fatir's controlled browser session. This browser is driven through Chrome DevTools, not mouse automation.", json!({"type":"object","properties":{"url":{"type":"string"}},"required":["url"],"additionalProperties":false})),
        tool("browser_get_url", "Read the URL of the Fatir-controlled browser tab directly from the browser automation connection.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("browser_tabs", "List all tabs/pages in Fatir's visible controlled browser and identify the selected tab. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("browser_new_tab", "Open a URL in a new visible Fatir browser tab. Use background=true only when the task explicitly benefits from keeping the current tab selected.", json!({"type":"object","properties":{"url":{"type":"string"},"background":{"type":"boolean"}},"required":["url"],"additionalProperties":false})),
        tool("browser_select_tab", "Select a Fatir browser tab by page ID from browser_tabs. Does not close any tabs.", json!({"type":"object","properties":{"page_id":{"type":"integer","minimum":0},"bring_to_front":{"type":"boolean"}},"required":["page_id"],"additionalProperties":false})),
        tool("browser_close_tab", "Close a Fatir browser tab by page ID. Never close the last tab. Use only when the task no longer needs that tab.", json!({"type":"object","properties":{"page_id":{"type":"integer","minimum":0}},"required":["page_id"],"additionalProperties":false})),
        tool("browser_extract_structure", "Extract structured page data without screenshots: headings, tables, lists, links, forms and repeated card-like items. Use this for research/comparison before visual inspection.", json!({"type":"object","properties":{"max_items":{"type":"integer","minimum":10,"maximum":300}},"additionalProperties":false})),
        tool("browser_network_recent", "Inspect recent network requests for the selected browser tab. Use failures_only=true to diagnose failed actions and API problems.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":200},"failures_only":{"type":"boolean"},"preserve":{"type":"boolean"}},"additionalProperties":false})),
        tool("browser_network_request", "Inspect one browser network request by request ID. Read-only. Fatir redacts cookies, authorization headers, API keys, tokens and secret-like values before returning it to the model.", json!({"type":"object","properties":{"request_id":{"type":"integer","minimum":0}},"required":["request_id"],"additionalProperties":false})),
        tool("browser_console_messages", "Inspect browser console output for the selected tab. By default return errors/warnings for diagnosing broken pages and failed scripts.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":200},"all_types":{"type":"boolean"},"preserve":{"type":"boolean"}},"additionalProperties":false})),
        tool("browser_diagnostics", "Run a compact browser diagnostic snapshot combining recent failed network activity and console errors/warnings for the selected tab. Read-only.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":5,"maximum":100}},"additionalProperties":false})),
        tool("browser_memory_current", "Read Fatir's learned semantic browser targets for the current site. Browser memory stores labels/roles and success history, never passwords or secret field values.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false})),
        tool("browser_takeover", "Pause Fatir computer control and hand the current browser step to the user, preserving the URL and reason. Use for CAPTCHA, unusual login verification, ambiguous sensitive confirmation, or when the user asks to take over.", json!({"type":"object","properties":{"reason":{"type":"string"}},"required":["reason"],"additionalProperties":false})),
        tool("browser_takeover_resume", "Resume Fatir computer control after the user has completed a takeover step. Re-observe/re-snapshot before acting because the page may have changed.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("browser_takeover_status", "Read whether Fatir computer control is paused for human takeover and the preserved browser checkpoint. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("headless_browser_tabs", "HEADLESS ONLY: list tabs in Fatir's isolated headless browser session. This tool is exposed only when the user explicitly requests headless execution.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("headless_browser_open_url", "HEADLESS ONLY: open/navigate a URL in Fatir's isolated headless browser. Never use unless the user's current request explicitly says headless.", json!({"type":"object","properties":{"url":{"type":"string"}},"required":["url"],"additionalProperties":false})),
        tool("headless_browser_new_tab", "HEADLESS ONLY: open a URL in a new isolated headless tab.", json!({"type":"object","properties":{"url":{"type":"string"},"background":{"type":"boolean"}},"required":["url"],"additionalProperties":false})),
        tool("headless_browser_select_tab", "HEADLESS ONLY: select a headless tab by page ID.", json!({"type":"object","properties":{"page_id":{"type":"integer","minimum":0}},"required":["page_id"],"additionalProperties":false})),
        tool("headless_browser_close_tab", "HEADLESS ONLY: close a headless tab by page ID.", json!({"type":"object","properties":{"page_id":{"type":"integer","minimum":0}},"required":["page_id"],"additionalProperties":false})),
        tool("headless_browser_elements", "HEADLESS ONLY: inspect the selected headless page accessibility snapshot and UIDs.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("headless_browser_click_text", "HEADLESS ONLY: click a semantic control in the selected headless page.", json!({"type":"object","properties":{"text":{"type":"string"},"exact":{"type":"boolean"}},"required":["text"],"additionalProperties":false})),
        tool("headless_browser_fill_by_label", "HEADLESS ONLY: fill an ordinary non-sensitive field in the selected headless page.", json!({"type":"object","properties":{"label":{"type":"string"},"text":{"type":"string"}},"required":["label","text"],"additionalProperties":false})),
        tool("headless_browser_extract_structure", "HEADLESS ONLY: extract structured headings/tables/lists/links/forms/cards from the selected headless page.", json!({"type":"object","properties":{"max_items":{"type":"integer","minimum":10,"maximum":300}},"additionalProperties":false})),
        tool("headless_browser_network_recent", "HEADLESS ONLY: inspect recent network requests in the selected headless page.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":200},"failures_only":{"type":"boolean"}},"additionalProperties":false})),
        tool("headless_browser_network_request", "HEADLESS ONLY: inspect one network request by request ID. Sensitive authentication/cookie headers are redacted before returning it to the model.", json!({"type":"object","properties":{"request_id":{"type":"integer","minimum":0}},"required":["request_id"],"additionalProperties":false})),
        tool("headless_browser_console_messages", "HEADLESS ONLY: inspect console errors/warnings in the selected headless page.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":200},"all_types":{"type":"boolean"}},"additionalProperties":false})),
        tool("headless_browser_observe", "HEADLESS ONLY: capture the selected headless page screenshot for visual inspection.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("headless_browser_stop", "HEADLESS ONLY: stop Fatir's isolated headless browser session and release its browser process. Use when the explicitly requested headless work is complete and no persistent session is needed.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("desktop_doctor", "Diagnose Fatir's generic Linux application-control stack: AT-SPI, X11 window control, keyboard/visual fallbacks, Java accessibility, and required local packages. Read-only and never opens a browser.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("desktop_repair_accessibility", "Repair the generic Linux application-control stack with one PolicyKit prompt: install AT-SPI/Python/Java/X11/visual dependencies, enable toolkit accessibility, and configure existing JetBrains-family IDE profiles. Requires approval; affected apps may need to be reopened afterward.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("desktop_apps", "List installed graphical Linux applications from system, user, Flatpak and Snap desktop launchers. Read-only. Use this to discover the exact installed app before launching it.", json!({"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":300}},"additionalProperties":false})),
        tool("desktop_windows", "List running desktop windows by merging AT-SPI accessibility applications with X11 window enumeration, so apps without accessibility metadata are still visible. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("desktop_capabilities", "Inspect how a specific running app/window can be controlled: semantic AT-SPI, keyboard/window fallback, screenshot vision, and X11 coordinate fallback. Read-only.", json!({"type":"object","properties":{"window":{"type":"string"}},"required":["window"],"additionalProperties":false})),
        tool("desktop_elements", "Inspect accessible controls in a desktop application window without moving the mouse. Returns element paths, roles, labels and actions. Prefer this for whole-desktop control.", json!({"type":"object","properties":{"window":{"type":"string"}},"required":["window"],"additionalProperties":false})),
        tool("desktop_find", "Find accessible desktop controls by human-readable label/description/role. Returns ranked matches with stable paths and screen bounds; never moves the mouse.", json!({"type":"object","properties":{"window":{"type":"string"},"query":{"type":"string"},"role":{"type":"string"}},"required":["window","query"],"additionalProperties":false})),
        tool("desktop_activate_named", "Move Fatir's independent visible desktop pointer to the best accessible control matching a human-readable query, then activate it through AT-SPI. The physical mouse is never moved.", json!({"type":"object","properties":{"window":{"type":"string"},"query":{"type":"string"},"role":{"type":"string"},"occurrence":{"type":"integer","minimum":0,"maximum":20}},"required":["window","query"],"additionalProperties":false})),
        tool("desktop_get_text", "Read text exposed by an accessible desktop text control. Refuses protected/password fields.", json!({"type":"object","properties":{"window":{"type":"string"},"path":{"type":"string"}},"required":["window","path"],"additionalProperties":false})),
        tool("desktop_wait_for", "Wait briefly for a desktop control to appear after an action, then return its ranked accessibility match. Use this to verify state changes instead of clicking repeatedly.", json!({"type":"object","properties":{"window":{"type":"string"},"query":{"type":"string"},"role":{"type":"string"},"timeout_seconds":{"type":"integer","minimum":1,"maximum":30}},"required":["window","query"],"additionalProperties":false})),
        tool("desktop_launch_app", "Launch an installed Linux desktop application by its human name or .desktop ID. Does not use the mouse.", json!({"type":"object","properties":{"app":{"type":"string"}},"required":["app"],"additionalProperties":false})),
        tool("desktop_fill_credential", "Fill an accessible desktop field using a stored Fatir credential without exposing the secret to the model. Requires explicit approval.", json!({"type":"object","properties":{"window":{"type":"string"},"path":{"type":"string"},"credential_id":{"type":"string"}},"required":["window","path","credential_id"],"additionalProperties":false})),
        tool("desktop_activate", "Move Fatir's independent visible desktop pointer to an accessible UI element, then activate it through AT-SPI. Does not move the physical mouse.", json!({"type":"object","properties":{"window":{"type":"string"},"path":{"type":"string"}},"required":["window","path"],"additionalProperties":false})),
        tool("desktop_set_text", "Set ordinary non-sensitive text in an accessible desktop text field without typing on the physical keyboard. Never use for passwords, OTPs, payment data or API keys; use the credential broker for stored secrets.", json!({"type":"object","properties":{"window":{"type":"string"},"path":{"type":"string"},"text":{"type":"string"}},"required":["window","path","text"],"additionalProperties":false})),
        tool("desktop_focus_window", "Bring a named desktop window to the front without moving the mouse.", json!({"type":"object","properties":{"window":{"type":"string"}},"required":["window"],"additionalProperties":false})),
        tool("desktop_key", "Send a keyboard shortcut to a named desktop window without moving the physical mouse. Generic fallback for any keyboard-accessible Linux app when AT-SPI is incomplete.", json!({"type":"object","properties":{"window":{"type":"string"},"keys":{"type":"string"}},"required":["window","keys"],"additionalProperties":false})),
        tool("desktop_type", "Type ordinary non-sensitive text into the currently focused control of a named desktop window without moving the physical mouse. Never use for passwords, OTPs, payment data, tokens or API keys.", json!({"type":"object","properties":{"window":{"type":"string"},"text":{"type":"string"}},"required":["window","text"],"additionalProperties":false})),
        tool("desktop_observe", "Capture a named desktop application window (or the current active window) for visual inspection. Use only after semantic/keyboard inspection is insufficient.", json!({"type":"object","properties":{"window":{"type":"string"}},"additionalProperties":false})),
        tool("desktop_visual_action", "LAST-RESORT X11 visual control for apps with no useful AT-SPI tree. Performs a screenshot-relative click, double-click, right-click, drag or scroll inside a named window, then restores the user's real mouse position. Prefer semantic AT-SPI and keyboard tools first. `purpose` must describe the intended control/action.", json!({"type":"object","properties":{"window":{"type":"string"},"action":{"type":"string","enum":["click","double_click","right_click","drag","scroll"]},"x":{"type":"integer","minimum":0},"y":{"type":"integer","minimum":0},"end_x":{"type":"integer","minimum":0},"end_y":{"type":"integer","minimum":0},"direction":{"type":"string","enum":["up","down","left","right"]},"amount":{"type":"integer","minimum":1,"maximum":30},"purpose":{"type":"string"}},"required":["window","action","x","y","purpose"],"additionalProperties":false})),
        tool("desktop_playbook", "Get Fatir V1 deterministic control hints/shortcuts for a known Linux application family, with generic fallback guidance. Read-only.", json!({"type":"object","properties":{"app":{"type":"string"}},"required":["app"],"additionalProperties":false})),
        tool("task_create", "Create persistent task state for multi-step work so Fatir can resume after the panel closes or the PC restarts.", json!({"type":"object","properties":{"title":{"type":"string"},"objective":{"type":"string"},"steps":{"type":"array","items":{"type":"string"}}},"required":["title","objective"],"additionalProperties":false})),
        tool("task_update", "Update a persistent Fatir task after a milestone, blocker, artifact or completion.", json!({"type":"object","properties":{"id":{"type":"string"},"status":{"type":"string"},"current_step":{"type":"string"},"completed_step":{"type":"string"},"note":{"type":"string"},"artifact":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("task_list", "List persistent Fatir tasks and their current state.", json!({"type":"object","properties":{"include_completed":{"type":"boolean"}},"additionalProperties":false})),
        tool("task_get", "Read full persistent state for one Fatir task.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("task_set_phase", "Move a persistent task through Fatir V1 phases: understand, plan, act, verify, recover, waiting, complete.", json!({"type":"object","properties":{"id":{"type":"string"},"phase":{"type":"string"}},"required":["id","phase"],"additionalProperties":false})),
        tool("task_checkpoint", "Record a verified milestone/evidence checkpoint for a persistent task.", json!({"type":"object","properties":{"id":{"type":"string"},"label":{"type":"string"},"evidence":{"type":"string"},"kind":{"type":"string"}},"required":["id","label"],"additionalProperties":false})),
        tool("task_record_error", "Record a concrete blocker/error on a persistent task and move it into recover state.", json!({"type":"object","properties":{"id":{"type":"string"},"error":{"type":"string"}},"required":["id","error"],"additionalProperties":false})),
        tool("task_resume", "Resume a persistent blocked/waiting task and increment its resume counter.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("project_inspect", "Inspect a local development/project folder: project type, Git branch/status, key manifests and recent commits. Read-only.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})),
        tool("project_remember", "Remember a local project path and label so future tasks can resume with project context. Stores metadata only, never source contents.", json!({"type":"object","properties":{"path":{"type":"string"},"label":{"type":"string"}},"required":["path"],"additionalProperties":false})),
        tool("project_list", "List local projects Fatir remembers. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("project_forget", "Forget one remembered project metadata entry. Does not delete project files.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})),
        tool("terminal_session_create", "Create a persistent logical terminal session with a remembered working directory and command history.", json!({"type":"object","properties":{"label":{"type":"string"},"cwd":{"type":"string"}},"required":["label"],"additionalProperties":false})),
        tool("terminal_session_list", "List Fatir persistent logical terminal sessions and their working directories. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("terminal_session_set_cwd", "Change the remembered working directory for a Fatir terminal session.", json!({"type":"object","properties":{"id":{"type":"string"},"cwd":{"type":"string"}},"required":["id","cwd"],"additionalProperties":false})),
        tool("terminal_session_exec", "Run a shell command inside a persistent logical terminal session, preserving cwd/history across turns. Use background_job_start for commands that must keep running after the turn.", json!({"type":"object","properties":{"id":{"type":"string"},"command":{"type":"string"},"timeout_seconds":{"type":"integer","minimum":1,"maximum":900}},"required":["id","command"],"additionalProperties":false})),
        tool("terminal_session_history", "Read recent command/output history for a persistent terminal session.", json!({"type":"object","properties":{"id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":80}},"required":["id"],"additionalProperties":false})),
        tool("terminal_session_close", "Close/forget a persistent logical terminal session. Does not kill unrelated processes.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("fatir_v1_status", "Read the V1 orchestration engine status and enabled safety/execution guarantees. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("background_job_start", "Start a long-running shell command as a persistent background job with its own log. Requires approval; use this for builds/downloads/tests that should continue after Fatir hides.", json!({"type":"object","properties":{"label":{"type":"string"},"command":{"type":"string"},"cwd":{"type":"string"}},"required":["label","command"],"additionalProperties":false})),
        tool("background_job_list", "List Fatir background jobs and whether they are still running.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("background_job_log", "Read the tail of a background job log.", json!({"type":"object","properties":{"id":{"type":"string"},"lines":{"type":"integer","minimum":1,"maximum":500}},"required":["id"],"additionalProperties":false})),
        tool("background_job_cancel", "Stop an Fatir background job. Requires approval.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("scheduled_job_create", "Create a persistent local systemd-user timer for a shell command. Browser/headless tools are never implied. Requires approval according to the shell policy.", json!({"type":"object","properties":{"label":{"type":"string"},"command":{"type":"string"},"cwd":{"type":"string"},"trigger_kind":{"type":"string","enum":["delay","calendar"]},"trigger":{"type":"string","description":"For delay use values like 10m/2h; for calendar use a systemd calendar expression."}},"required":["label","command","trigger_kind","trigger"],"additionalProperties":false})),
        tool("scheduled_job_list", "List persistent local Fatir schedules and timer state. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("scheduled_job_cancel", "Cancel and forget a Fatir local scheduled command. Requires approval according to background-job policy.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("agent_schedule_create", "Create a persistent scheduled Fatir agent task that can wake the model and, when explicitly authorized here, use managed/current/headless browser mode and selected stored credentials. Creation requires approval because it grants future autonomous execution.", json!({"type":"object","properties":{"label":{"type":"string"},"prompt":{"type":"string"},"trigger_kind":{"type":"string","enum":["delay","calendar"]},"trigger":{"type":"string"},"browser_mode":{"type":"string","enum":["none","managed","headless","current"]},"credential_ids":{"type":"array","items":{"type":"string"},"maxItems":20}},"required":["label","prompt","trigger_kind","trigger","browser_mode"],"additionalProperties":false})),
        tool("agent_schedule_list", "List scheduled Fatir agent/web tasks, browser authorization, next run and last result. Read-only.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("agent_schedule_cancel", "Cancel and remove a scheduled Fatir agent/web task. Requires approval.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("checkpoint_files", "Create rollback checkpoints for local files before editing configuration or running a potentially mutating command. Read-only backup creation; supports files up to 100 MB each.", json!({"type":"object","properties":{"label":{"type":"string"},"paths":{"type":"array","items":{"type":"string"},"maxItems":12}},"required":["label","paths"],"additionalProperties":false})),
        tool("rollback_list", "List available local rollback points created by Fatir.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false})),
        tool("rollback_execute", "Restore one Fatir rollback point. Requires approval because it changes the current filesystem/software state.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("software_inventory", "Inspect installed software and identify installation source (APT, Flatpak, AppImage). Optionally filter by name.", json!({"type":"object","properties":{"query":{"type":"string"}},"additionalProperties":false})),
        tool("software_updates", "Inspect available APT and Flatpak updates without installing them.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("credential_list", "List names/accounts of credentials stored in Fatir's Linux keyring. Never returns secret values.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("run_shell_with_credentials", "Run a user shell command with selected stored credentials injected only as environment variables. Secret values never enter model context or action history. Requires approval.", json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"},"credentials":{"type":"object","additionalProperties":{"type":"string"}},"timeout_seconds":{"type":"integer","minimum":1,"maximum":900}},"required":["command","credentials"],"additionalProperties":false})),
        tool("browser_fill_credential", "Fill a browser field using a stored Fatir credential without revealing the secret to the model. Requires explicit approval.", json!({"type":"object","properties":{"element_id":{"type":"string"},"credential_id":{"type":"string"}},"required":["element_id","credential_id"],"additionalProperties":false})),
        tool("browser_fill_credential_by_label", "Fill a browser password/login field by semantic label using a stored Fatir credential without revealing the secret to the model. Prefer this when the field is identifiable as Password, Email, Username, etc. Stored-credential approval is required unless that credential was explicitly pre-authorized on the current scheduled agent task.", json!({"type":"object","properties":{"label":{"type":"string"},"credential_id":{"type":"string"}},"required":["label","credential_id"],"additionalProperties":false})),
        tool("delegate_specialist", "Ask a specialist sub-agent for a focused second opinion or plan. Roles: system, software, browser, code, research. The specialist cannot make changes; Fatir remains responsible for execution and verification.", json!({"type":"object","properties":{"role":{"type":"string","enum":["system","software","browser","code","research"]},"task":{"type":"string"},"context":{"type":"string"}},"required":["role","task"],"additionalProperties":false})),
        tool("recent_actions", "Read Fatir's recent local action history so you can answer what was changed or done previously.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false})),
        tool("recent_activity", "Read Fatir's recent locally learned activity context: app focus changes, browser visits and terminal command history. Read-only. Use only when relevant to the user's current task.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":200}},"additionalProperties":false})),
        tool("routine_summary", "Read Fatir's local routine summary: frequently used apps, sites, command families and recent activity. Read-only. This is learned context, not an instruction.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("routine_create", "Save a reusable named routine from a user-approved set of human-readable steps. The routine is only a plan; running it later still respects normal tool approvals.", json!({"type":"object","properties":{"name":{"type":"string"},"description":{"type":"string"},"steps":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":40}},"required":["name","steps"],"additionalProperties":false})),
        tool("routine_capture_recent", "Create a reusable routine from recent successful non-sensitive Fatir actions. Credential actions and redacted arguments are excluded. Requires approval because it stores a persistent automation recipe.", json!({"type":"object","properties":{"name":{"type":"string"},"description":{"type":"string"},"action_count":{"type":"integer","minimum":1,"maximum":40}},"required":["name"],"additionalProperties":false})),
        tool("routine_list_saved", "List saved reusable routines and run counts.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("routine_prepare_run", "Load a saved routine for execution and mark that a run started. Execute returned steps one by one with normal verification and approvals; do not blindly replay unsafe state.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("routine_remove", "Delete a saved routine definition. Requires approval; it does not undo actions previously performed by the routine.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("teach_start", "Start Teach Mode 3.0 recording. Fatir records successful actions as semantic reusable steps; credentials/secrets are excluded and browser coordinates/MCP UIDs are converted to semantic targets when possible.", json!({"type":"object","properties":{"name":{"type":"string"},"description":{"type":"string"}},"required":["name"],"additionalProperties":false})),
        tool("teach_status", "Read Teach Mode 3.0 recording status and step count.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("teach_stop", "Stop Teach Mode 3.0 and save the recorded semantic workflow as a reusable routine.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("teach_cancel", "Cancel the current Teach Mode 3.0 recording without saving it.", json!({"type":"object","properties":{},"additionalProperties":false})),
        tool("proactive_events", "Read unacknowledged local proactive events such as disk pressure, cleanup opportunities, or completed/failed background jobs.", json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false})),
        tool("proactive_ack", "Acknowledge one proactive event after it has been handled or dismissed.", json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false})),
        tool("run_shell_command", "Run a general Bash command as the current user and return stdout/stderr/exit status. Use this when dedicated tools do not cover the task, including curl, chmod, mv, git, npm, cargo or installer scripts. Harmless user-level commands follow the approval preference; destructive/system-sensitive commands still require approval. Do not include sudo or pkexec; use run_privileged_command if root is truly required.", json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"},"timeout_seconds":{"type":"integer","minimum":1,"maximum":900}},"required":["command"],"additionalProperties":false})),
        tool("run_privileged_command", "Run a Bash command through Linux PolicyKit (pkexec) after explicit user approval. Use only when root access is genuinely required. Do not include sudo in the command. Fatir never receives the user's password.", json!({"type":"object","properties":{"command":{"type":"string"},"timeout_seconds":{"type":"integer","minimum":1,"maximum":900}},"required":["command"],"additionalProperties":false})),
        tool("web_search", "Search the current web using Ollama web search. Use for official installers, current documentation and changing facts.", json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false})),
        tool("download_file", "Download a file from a URL into the user's Downloads/Fatir folder. Requires confirmation.", json!({"type":"object","properties":{"url":{"type":"string"},"filename":{"type":"string"}},"required":["url","filename"],"additionalProperties":false})),
        tool("install_apt", "Install an APT package using the normal Linux PolicyKit authentication dialog. Requires confirmation.", json!({"type":"object","properties":{"package":{"type":"string"}},"required":["package"],"additionalProperties":false})),
        tool("install_flatpak", "Install a Flatpak application for the current user. Requires confirmation.", json!({"type":"object","properties":{"app_id":{"type":"string"}},"required":["app_id"],"additionalProperties":false})),
        tool("install_deb", "Install a local .deb package using PolicyKit. Requires confirmation.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})),
        tool("extract_archive", "Extract a zip/tar archive into a destination folder. Requires confirmation.", json!({"type":"object","properties":{"archive":{"type":"string"},"destination":{"type":"string"}},"required":["archive","destination"],"additionalProperties":false})),
        tool("create_desktop_entry", "Create a user application launcher under ~/.local/share/applications. Requires confirmation.", json!({"type":"object","properties":{"name":{"type":"string"},"exec":{"type":"string"},"icon":{"type":"string"}},"required":["name","exec"],"additionalProperties":false})),
        tool("move_to_trash", "Move a local file or folder to the desktop Trash instead of permanently deleting it. Requires confirmation.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false})),
    ]
}

fn tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({"type":"function","function":{"name":name,"description":description,"parameters":parameters}})
}

pub fn tool_definitions_for_hint(hint: &str) -> Vec<Value> {
    let h = hint.to_lowercase();
    let browser_request = adaptive::is_browser_request(hint);
    let explicit_headless = adaptive::explicit_headless(hint);
    let select = |allowed: HashSet<&str>| {
        tool_definitions().into_iter().filter(|v| {
            v.pointer("/function/name").and_then(Value::as_str).map(|n| allowed.contains(n)).unwrap_or(false)
        }).collect::<Vec<_>>()
    };

    // Fast paths keep the schema tiny for common browser work. This matters because
    // every tool schema is part of the model input on every agent turn.
    if !explicit_headless && h.contains("amazon") && ["keyword", "search", "asin", "bullet", "seller", "price", "csv", "top 5", "top five"].iter().any(|w| h.contains(w)) {
        return select(["amazon_keyword_research", "open_path"].into_iter().collect());
    }
    if !explicit_headless && browser_request && (h.contains("continue with google") || h.contains("sign in with google") || h.contains("login with google")) {
        return select(["browser_open_url", "browser_click_text", "browser_page_summary", "browser_get_url", "browser_console_messages", "browser_network_recent", "browser_takeover", "browser_takeover_status", "credential_list", "browser_fill_credential", "browser_fill_credential_by_label"].into_iter().collect());
    }

    let mut allowed: HashSet<&str> = [
        "system_snapshot", "health_report", "fatir_v1_status", "task_create", "task_update", "task_list", "task_get", "task_set_phase", "task_checkpoint", "task_record_error", "task_resume",
        "background_job_start", "background_job_list", "background_job_log", "background_job_cancel", "scheduled_job_list",
        "recent_actions", "recent_activity", "routine_summary", "delegate_specialist", "project_list", "terminal_session_list",
        "open_path", "run_shell_command", "run_privileged_command", "desktop_doctor"
    ].into_iter().collect();

    let any = |words: &[&str]| words.iter().any(|w| h.contains(*w));
    let takeover_control = any(&["resume fatir", "resume control", "resume browser control", "takeover status", "take over status", "takeover complete", "manual step done"]);
    if takeover_control || takeover_path().is_file() { allowed.extend(["browser_takeover_resume","browser_takeover_status"]); }
    if teach::is_active() { allowed.extend(["teach_status","teach_stop","teach_cancel"]); }
    if browser_request && !explicit_headless {
        allowed.extend([
            "browser_open_url","browser_get_url","browser_page_summary","browser_click_text","browser_fill_by_label",
            "browser_elements","browser_click_element","browser_fill_element","browser_fill_credential","browser_fill_credential_by_label","browser_upload_file",
            "browser_key","browser_scroll","browser_observe","browser_click","browser_type","web_search","share_whatsapp_send","share_email_draft","share_latest_screenshot",
            "browser_tabs","browser_new_tab","browser_select_tab","browser_close_tab","browser_extract_structure",
            "browser_network_recent","browser_network_request","browser_console_messages","browser_diagnostics","browser_memory_current",
            "browser_takeover","browser_takeover_resume","browser_takeover_status"
        ]);
    }
    if explicit_headless {
        allowed.extend([
            "headless_browser_tabs","headless_browser_open_url","headless_browser_new_tab","headless_browser_select_tab","headless_browser_close_tab",
            "headless_browser_elements","headless_browser_click_text","headless_browser_fill_by_label","headless_browser_extract_structure",
            "headless_browser_network_recent","headless_browser_network_request","headless_browser_console_messages","headless_browser_observe","headless_browser_stop","web_search"
        ]);
    }
    if !explicit_headless && h.contains("amazon") { allowed.insert("amazon_keyword_research"); }
    if any(&["file","folder","pdf","document","downloads","download","csv","image","screenshot","largest","storage","disk","duplicate","trash","recycle","cleanup","clean up","space"]) {
        allowed.extend([
            "list_directory","list_largest_files","read_file","render_pdf_pages","write_text_file","copy_file","move_path",
            "create_directory","take_screenshot","share_latest_screenshot","download_file","extract_archive","cleanup_scan","duplicate_scan",
            "trash_inventory","cleanup_move_to_trash","empty_trash","move_to_trash","checkpoint_files","rollback_list","rollback_execute"
        ]);
    }
    if any(&["install","update","software","package","apt","flatpak","deb","appimage","cli","command","terminal","build","compile","npm","cargo","wordpress","wp-cli","composer"]) {
        allowed.extend([
            "software_inventory","software_updates","install_apt","install_flatpak","install_deb","download_file","extract_archive",
            "create_desktop_entry","run_shell_command","run_privileged_command","run_shell_with_credentials","checkpoint_files","rollback_list","rollback_execute",
            "terminal_session_create","terminal_session_list","terminal_session_set_cwd","terminal_session_exec","terminal_session_history","terminal_session_close",
            "project_inspect","project_remember","project_list","project_forget","scheduled_job_create","scheduled_job_list","scheduled_job_cancel"
        ]);
    }
    if any(&["desktop","window","app","android studio","settings","dialog","button","control","screen","at-spi","pyatspi","accessibility","work locally","local application","local apps","installed application","installed applications"])
        || (!browser_request && ["open ","launch ","start ","use ","control "].iter().any(|p| h.trim_start().starts_with(p))) {
        allowed.extend([
            "desktop_doctor","desktop_repair_accessibility","desktop_apps","desktop_windows","desktop_capabilities","desktop_elements","desktop_find","desktop_activate_named","desktop_get_text","desktop_wait_for",
            "desktop_launch_app","desktop_activate","desktop_set_text","desktop_focus_window","desktop_observe","desktop_visual_action","desktop_fill_credential","desktop_key","desktop_type","desktop_playbook"
        ]);
    }
    if any(&["project","repo","repository","git","workspace","codebase","source tree","gradle project","android project","rust project","node project"]) {
        allowed.extend(["project_inspect","project_remember","project_list","project_forget","terminal_session_create","terminal_session_list","terminal_session_set_cwd","terminal_session_exec","terminal_session_history"]);
    }
    if any(&["schedule","scheduled","scheduler","daily","weekly","monthly","every day","every week","every month","remind me","reminder","timer","in 10 minutes","in 30 minutes","in an hour"]) {
        allowed.extend(["scheduled_job_create","scheduled_job_list","scheduled_job_cancel","agent_schedule_create","agent_schedule_list","agent_schedule_cancel","background_job_start","background_job_list","background_job_log","background_job_cancel"]);
    }
    if any(&["routine","workflow","again","as before","repeat","automation","automate","teach mode","teach me","record this workflow","record workflow","record task"]) {
        allowed.extend(["routine_create","routine_capture_recent","routine_list_saved","routine_prepare_run","routine_remove","teach_start","teach_status","teach_stop","teach_cancel"]);
    }
    if any(&["credential","secret","token","keyring","api key","login","log in","sign in","password","wordpress","wp-admin","gmail","google"]) {
        allowed.extend(["credential_list","browser_fill_credential","browser_fill_credential_by_label","desktop_fill_credential","run_shell_with_credentials"]);
    }
    if any(&["alert","proactive","attention","monitor"]) {
        allowed.extend(["proactive_events","proactive_ack"]);
    }
    if any(&["research","latest","online","web search"]) { allowed.insert("web_search"); }

    select(allowed)
}

pub fn tool_definitions_for_local_hint(hint: &str) -> Vec<Value> {
    let h = hint.to_lowercase();
    let select = |allowed: HashSet<&str>| {
        tool_definitions().into_iter().filter(|v| {
            v.pointer("/function/name").and_then(Value::as_str).map(|n| allowed.contains(n)).unwrap_or(false)
        }).collect::<Vec<_>>()
    };
    let any = |words: &[&str]| words.iter().any(|w| h.contains(*w));
    let mut allowed: HashSet<&str> = ["system_snapshot", "health_report", "run_shell_command", "open_path", "desktop_doctor"].into_iter().collect();

    if any(&["install","update","software","package","apt","flatpak","deb","appimage","cli","composer","wp-cli"]) {
        allowed.extend(["software_inventory","software_updates","install_apt","install_flatpak","install_deb","download_file","extract_archive","run_shell_command","run_privileged_command","checkpoint_files"]);
    }
    if any(&["file","folder","directory","pdf","document","downloads","largest","storage","disk","duplicate","trash","cleanup","space","move","copy","rename","archive"]) {
        allowed.extend(["list_directory","list_largest_files","list_largest_directories","read_file","cleanup_scan","duplicate_scan","trash_inventory","cleanup_move_to_trash","empty_trash","move_to_trash","write_text_file","copy_file","move_path","create_directory","checkpoint_files"]);
    }
    if any(&["service","systemctl","process","cpu","ram","memory","mount","network","port","permission","terminal","command","shell"]) {
        allowed.extend(["system_snapshot","health_report","run_shell_command","run_privileged_command","background_job_start","background_job_list","background_job_log","terminal_session_create","terminal_session_list","terminal_session_set_cwd","terminal_session_exec","terminal_session_history","terminal_session_close"]);
    }
    if any(&["schedule","scheduled","scheduler","daily","weekly","monthly","every day","every week","every month","remind me","reminder","timer"]) {
        allowed.extend(["scheduled_job_create","scheduled_job_list","scheduled_job_cancel","agent_schedule_create","agent_schedule_list","agent_schedule_cancel"]);
    }
    if any(&["desktop","window","app","android studio","settings","dialog","button","control","screen","at-spi","accessibility","installed application","installed applications"])
        || ["open ","launch ","start ","use ","control "].iter().any(|p| h.trim_start().starts_with(p)) {
        allowed.extend(["desktop_doctor","desktop_repair_accessibility","desktop_apps","desktop_windows","desktop_capabilities","desktop_elements","desktop_find","desktop_activate_named","desktop_get_text","desktop_wait_for","desktop_launch_app","desktop_activate","desktop_set_text","desktop_focus_window","desktop_observe","desktop_visual_action","desktop_fill_credential","desktop_key","desktop_type","desktop_playbook"]);
    }
    if any(&["project","repo","repository","git","workspace","codebase","gradle","android project","rust project","node project"]) {
        allowed.extend(["project_inspect","project_remember","project_list","project_forget","terminal_session_create","terminal_session_list","terminal_session_set_cwd","terminal_session_exec","terminal_session_history"]);
    }
    if any(&["routine","workflow","again","repeat"]) {
        allowed.extend(["routine_list_saved","routine_prepare_run","routine_capture_recent"]);
    }
    select(allowed)
}

pub fn tool_definitions_for_router_hint(hint: &str) -> Vec<Value> {
    let h = hint.to_lowercase();
    let select = |allowed: HashSet<&str>| {
        tool_definitions().into_iter().filter(|v| {
            v.pointer("/function/name").and_then(Value::as_str).map(|n| allowed.contains(n)).unwrap_or(false)
        }).collect::<Vec<_>>()
    };
    let any = |words: &[&str]| words.iter().any(|w| h.contains(*w));
    let mut allowed: HashSet<&str> = HashSet::new();
    if any(&["largest folder","largest folders","largest directory","largest directories","biggest folder","biggest folders"]) {
        allowed.insert("list_largest_directories");
    } else if any(&["largest file","largest files","big file","big files"]) {
        allowed.insert("list_largest_files");
    } else if any(&["disk","cpu","ram","memory","system info","system health","health"]) {
        allowed.extend(["system_snapshot","health_report"]);
    } else if any(&["trash","recycle bin"]) {
        allowed.extend(["trash_inventory","empty_trash"]);
    } else if any(&["downloads","clean up","cleanup","duplicate","space"]) {
        allowed.extend(["cleanup_scan","duplicate_scan","trash_inventory"]);
    } else if any(&["list folder","list directory","show files","show folder"]) {
        allowed.insert("list_directory");
    } else if any(&["open file","open folder","open path"]) {
        allowed.insert("open_path");
    } else if any(&["install","package","apt","flatpak","deb"]) {
        allowed.extend(["software_inventory","install_apt","install_flatpak","install_deb"]);
    } else if any(&["desktop","desktop app","desktop application","open apps","open applications","window","windows","computer control","control app","button","dialog","settings","android studio","at-spi","pyatspi","accessibility","work locally","installed application","installed applications"])
        || ["open ","launch ","start ","use ","control "].iter().any(|p| h.trim_start().starts_with(p)) {
        allowed.extend([
            "desktop_doctor","desktop_repair_accessibility","desktop_apps","desktop_windows","desktop_capabilities","desktop_elements","desktop_find","desktop_activate_named",
            "desktop_get_text","desktop_wait_for","desktop_launch_app","desktop_focus_window","desktop_key","desktop_type","desktop_observe","desktop_visual_action"
        ]);
    } else if any(&["schedule","scheduled","scheduler","daily","weekly","monthly","every day","every week","every month","remind me","reminder","timer"]) {
        allowed.extend(["scheduled_job_create","scheduled_job_list","scheduled_job_cancel","agent_schedule_create","agent_schedule_list","agent_schedule_cancel"]);
    } else if any(&["update software","updates","update packages"]) {
        allowed.insert("software_updates");
    }
    if allowed.is_empty() {
        // No broad shell fallback here. The router must escalate rather than
        // inventing arbitrary commands or turning into a slow general agent.
        allowed.extend(["system_snapshot","health_report"]);
    }
    select(allowed)
}

fn desktop_action_risk(args:&Value) -> &'static str {
    let text=["query","purpose"].iter().filter_map(|k|args.get(*k).and_then(Value::as_str)).collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    let keys=args.get("keys").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
    if ["delete","remove","uninstall","erase","format","factory reset","empty trash","permanently","permanent delete","wipe"].iter().any(|x|text.contains(x))
        || keys.contains("shift+delete") || keys.contains("shift+kp_delete") { return "destructive"; }
    if ["buy now","purchase","place order","pay now","submit payment","change password","security setting"].iter().any(|x|text.contains(x)) { return "change"; }
    "auto"
}


fn shell_command_risk(command:&str)-> &'static str {
    let lower=command.to_ascii_lowercase();
    let destructive=[
        r"(^|[;&|]{1,2}\s*)(rm|rmdir|shred|wipefs|mkfs(?:\.[a-z0-9_+-]+)?|fdisk|cfdisk|sfdisk|parted)(\s|$)",
        r"\b(apt|apt-get|dnf|yum)\s+(remove|purge|autoremove)\b",
        r"\bpacman\s+-r", r"\bflatpak\s+uninstall\b", r"\bsnap\s+remove\b",
        r"\bgit\s+(reset\s+--hard|clean\s+-[a-z]*f)", r"\bdocker\s+(system|volume|image|container)\s+prune\b",
        r"\btruncate\s+-s\s*0\b", r"(^|[;&|]{1,2}\s*)dd\s+.*\bof=/dev/",
    ];
    if destructive.iter().any(|p|Regex::new(p).map(|r|r.is_match(&lower)).unwrap_or(false)){return "destructive";}
    if lower.contains("pkexec") || Regex::new(r"(^|[;&|]{1,2}\s*)(mount|umount|chown)(\s|$)").map(|r|r.is_match(&lower)).unwrap_or(false){return "system";}
    "change"
}

pub fn risk_for(tool: &str, args:&Value) -> &'static str {
    match tool {
        "install_apt" | "install_deb" | "run_privileged_command" | "desktop_repair_accessibility" => "system",
        "browser_fill_credential" | "browser_fill_credential_by_label" | "desktop_fill_credential" => "sensitive",
        "run_shell_with_credentials" | "agent_schedule_create" => "sensitive",
        "run_shell_command" | "terminal_session_exec" | "background_job_start" | "scheduled_job_create" => shell_command_risk(args.get("command").and_then(Value::as_str).unwrap_or("")),
        "install_flatpak" | "download_file" | "extract_archive" | "create_desktop_entry" | "background_job_cancel" | "scheduled_job_cancel" | "agent_schedule_cancel" | "write_text_file" | "copy_file" | "move_path" | "create_directory" | "routine_capture_recent" | "routine_remove" | "project_forget" | "terminal_session_close" => "change",
        "move_to_trash" | "cleanup_move_to_trash" | "empty_trash" => "destructive",
        "rollback_execute" => "change",
        "desktop_activate_named" | "desktop_visual_action" | "desktop_key" => desktop_action_risk(args),
        _ => "auto",
    }
}

pub fn summary_for(tool: &str, args: &Value) -> String {
    match tool {
        "install_apt" => format!("Install APT package {}", args.get("package").and_then(Value::as_str).unwrap_or("")),
        "install_flatpak" => format!("Install Flatpak {}", args.get("app_id").and_then(Value::as_str).unwrap_or("")),
        "install_deb" => format!("Install {}", args.get("path").and_then(Value::as_str).unwrap_or("package")),
        "download_file" => format!("Download {}", args.get("filename").and_then(Value::as_str).unwrap_or("file")),
        "extract_archive" => format!("Extract {}", args.get("archive").and_then(Value::as_str).unwrap_or("archive")),
        "create_desktop_entry" => format!("Create launcher for {}", args.get("name").and_then(Value::as_str).unwrap_or("application")),
        "move_to_trash" => format!("Move {} to Trash", args.get("path").and_then(Value::as_str).unwrap_or("item")),
        "write_text_file" => format!("Write {}", args.get("path").and_then(Value::as_str).unwrap_or("file")),
        "copy_file" => format!("Copy to {}", args.get("destination").and_then(Value::as_str).unwrap_or("destination")),
        "move_path" => format!("Move to {}", args.get("destination").and_then(Value::as_str).unwrap_or("destination")),
        "create_directory" => format!("Create folder {}", args.get("path").and_then(Value::as_str).unwrap_or("folder")),
        "run_shell_command" => format!("Run command: {}", shorten(args.get("command").and_then(Value::as_str).unwrap_or(""), 140)),
        "run_privileged_command" => format!("Run as administrator: {}", shorten(args.get("command").and_then(Value::as_str).unwrap_or(""), 140)),
        "run_shell_with_credentials" => format!("Run command with stored credentials: {}", shorten(args.get("command").and_then(Value::as_str).unwrap_or(""), 140)),
        "browser_fill_credential" | "browser_fill_credential_by_label" => "Use a stored credential in the browser".into(),
        "browser_upload_file" => {
            let path=args.get("path").and_then(Value::as_str).unwrap_or("file");
            let name=Path::new(path).file_name().and_then(|x|x.to_str()).unwrap_or(path);
            format!("Upload local file {name} to the current webpage")
        },
        "share_whatsapp_send" => format!("Send {} local file(s) to {} on WhatsApp", args.get("paths").and_then(Value::as_array).map(|x|x.len()).unwrap_or(0), args.get("contact").and_then(Value::as_str).unwrap_or("contact")),
        "share_email_draft" => format!("Attach {} local file(s) to an email draft for {}", args.get("paths").and_then(Value::as_array).map(|x|x.len()).unwrap_or(0), args.get("to").and_then(Value::as_str).unwrap_or("recipient")),
        "desktop_fill_credential" => "Use a stored credential in a desktop application".into(),
        "desktop_repair_accessibility" => "Repair generic Linux desktop application control".into(),
        "desktop_activate_named" => format!("Activate {} in {}", args.get("query").and_then(Value::as_str).unwrap_or("control"), args.get("window").and_then(Value::as_str).unwrap_or("desktop app")),
        "desktop_key" => format!("Send {} to {}", args.get("keys").and_then(Value::as_str).unwrap_or("keyboard shortcut"), args.get("window").and_then(Value::as_str).unwrap_or("desktop app")),
        "desktop_visual_action" => format!("{} in {}", args.get("purpose").and_then(Value::as_str).unwrap_or("Perform visual desktop action"), args.get("window").and_then(Value::as_str).unwrap_or("desktop app")),
        "cleanup_move_to_trash" => format!("Move {} selected item(s) to Trash", args.get("paths").and_then(Value::as_array).map(|x|x.len()).unwrap_or(0)),
        "empty_trash" => "Permanently empty desktop Trash".into(),
        "routine_capture_recent" => format!("Save recent actions as routine {}", args.get("name").and_then(Value::as_str).unwrap_or("routine")),
        "routine_remove" => format!("Remove saved routine {}", args.get("id").and_then(Value::as_str).unwrap_or("")),
        "background_job_start" => format!("Start background job: {}", args.get("label").and_then(Value::as_str).unwrap_or("job")),
        "agent_schedule_create" => {
            let label=args.get("label").and_then(Value::as_str).unwrap_or("agent task");
            let browser=args.get("browser_mode").and_then(Value::as_str).unwrap_or("none");
            let credentials=args.get("credential_ids").and_then(Value::as_array).map(|v|v.len()).unwrap_or(0);
            format!("Create scheduled agent task: {label} · browser: {browser} · {credentials} stored credential(s) authorized")
        },
        "agent_schedule_cancel" => format!("Cancel scheduled agent task {}", args.get("id").and_then(Value::as_str).unwrap_or("")),
        "background_job_cancel" => format!("Stop background job {}", args.get("id").and_then(Value::as_str).unwrap_or("")),
        "terminal_session_exec" => format!("Run in terminal session: {}", shorten(args.get("command").and_then(Value::as_str).unwrap_or(""),140)),
        "project_forget" => format!("Forget project metadata {}", args.get("path").and_then(Value::as_str).unwrap_or("")),
        "rollback_execute" => format!("Restore rollback point {}", args.get("id").and_then(Value::as_str).unwrap_or("")),
        _ => tool.replace('_', " "),
    }
}

pub async fn execute(tool: &str, args: &Value, api_key: Option<&str>) -> Result<(String, TraceItem)> {
    // One logical Fatir cursor owns exactly one control surface at a time.
    // Browser actions hide the desktop overlay; desktop actions hide the in-page browser pointer.
    if tool.starts_with("browser_") || tool == "amazon_keyword_research" {
        let _ = pointer::enter_browser();
    } else if tool.starts_with("desktop_") {
        let _ = browser_cursor_hide().await;
        let _ = pointer::hide();
    }
    let result = match tool {
        "system_snapshot" => serde_json::to_string_pretty(&system_snapshot().await?)?,
        "health_report" => serde_json::to_string_pretty(&crate::health::report().await?)?,
        "list_directory" => list_directory(required_str(args, "path")?)?,
        "list_largest_files" => {
            let path = required_str(args, "path")?;
            let min = args.get("minimum_size_mb").and_then(Value::as_u64).unwrap_or(100);
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(40) as usize;
            serde_json::to_string_pretty(&largest_files(path, min, limit)?)?
        }
        "list_largest_directories" => {
            let path = required_str(args, "path")?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize;
            largest_directories(path, limit).await?
        }
        "cleanup_scan" => serde_json::to_string_pretty(&cleanup::scan(args.get("path").and_then(Value::as_str), args.get("min_age_days").and_then(Value::as_u64).unwrap_or(14), args.get("limit").and_then(Value::as_u64).unwrap_or(120) as usize)?)?,
        "duplicate_scan" => serde_json::to_string_pretty(&cleanup::duplicate_scan(args.get("path").and_then(Value::as_str), args.get("minimum_size_mb").and_then(Value::as_u64).unwrap_or(5), args.get("limit_groups").and_then(Value::as_u64).unwrap_or(40) as usize)?)?,
        "trash_inventory" => serde_json::to_string_pretty(&cleanup::trash_summary(args.get("limit").and_then(Value::as_u64).unwrap_or(80) as usize)?)?,
        "cleanup_move_to_trash" => { let paths=args.get("paths").and_then(Value::as_array).ok_or_else(||anyhow!("Missing paths"))?.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>(); serde_json::to_string_pretty(&cleanup::move_many_to_trash(&paths)?)? },
        "empty_trash" => serde_json::to_string_pretty(&cleanup::empty_trash()?)?,
        "read_file" => read_file(required_str(args, "path")?).await?,
        "render_pdf_pages" => render_pdf_pages(required_str(args, "path")?, args.get("start_page").and_then(Value::as_u64).unwrap_or(1) as usize, args.get("end_page").and_then(Value::as_u64).unwrap_or(1) as usize).await?,
        "open_path" => open_path(required_str(args, "path")?).await?,
        "write_text_file" => write_text_file(required_str(args,"path")?, required_str(args,"content")?, args.get("create_parents").and_then(Value::as_bool).unwrap_or(true))?,
        "copy_file" => copy_file(required_str(args,"source")?, required_str(args,"destination")?)?,
        "move_path" => move_path(required_str(args,"source")?, required_str(args,"destination")?)?,
        "create_directory" => create_directory(required_str(args,"path")?)?,
        "take_screenshot" => take_screenshot().await?,
        "browser_observe" => browser_observe().await?,
        "browser_elements" => browser_elements().await?,
        "browser_click_text" => browser_click_text(required_str(args, "text")?, args.get("exact").and_then(Value::as_bool).unwrap_or(false)).await?,
        "browser_fill_by_label" => browser_fill_by_label(required_str(args, "label")?, required_str(args, "text")?).await?,
        "browser_upload_file" => browser_upload_file(required_str(args, "path")?, args.get("hint").and_then(Value::as_str)).await?,
        "share_whatsapp_send" => { let paths=args.get("paths").and_then(Value::as_array).ok_or_else(||anyhow!("Missing paths"))?.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>(); serde_json::to_string_pretty(&share::whatsapp_send(&paths, required_str(args,"contact")?).await?)? },
        "share_email_draft" => { let paths=args.get("paths").and_then(Value::as_array).ok_or_else(||anyhow!("Missing paths"))?.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>(); serde_json::to_string_pretty(&share::email_draft(&paths, required_str(args,"to")?, args.get("subject").and_then(Value::as_str).unwrap_or("")).await?)? },
        "share_latest_screenshot" => serde_json::to_string_pretty(&share::latest_screenshot()?)?,
        "browser_page_summary" => browser_page_summary(args.get("max_chars").and_then(Value::as_u64).unwrap_or(8000) as usize).await?,
        "amazon_keyword_research" => amazon_keyword_research(required_str(args, "keyword")?, args.get("limit").and_then(Value::as_u64).unwrap_or(5) as usize, args.get("output_path").and_then(Value::as_str)).await?,
        "browser_click_element" => browser_click_element(required_str(args, "element_id")?).await?,
        "browser_fill_element" => browser_fill_element(required_str(args, "element_id")?, required_str(args, "text")?).await?,
        "browser_click" => browser_click(args.get("x").and_then(Value::as_i64).ok_or_else(|| anyhow!("Missing x"))?, args.get("y").and_then(Value::as_i64).ok_or_else(|| anyhow!("Missing y"))?).await?,
        "browser_type" => browser_type(required_str(args, "text")?).await?,
        "browser_key" => browser_key(required_str(args, "key")?).await?,
        "browser_scroll" => browser_scroll(required_str(args, "direction")?, args.get("steps").and_then(Value::as_u64).unwrap_or(5) as usize).await?,
        "browser_open_url" => browser_open_url(required_str(args, "url")?).await?,
        "browser_get_url" => browser_get_url().await?,
        "browser_tabs" => mcp_tabs(false).await?,
        "browser_new_tab" => mcp_new_tab(false, required_str(args,"url")?, args.get("background").and_then(Value::as_bool).unwrap_or(false)).await?,
        "browser_select_tab" => mcp_select_tab(false, args.get("page_id").and_then(Value::as_u64).ok_or_else(||anyhow!("Missing page_id"))? as u32, args.get("bring_to_front").and_then(Value::as_bool).unwrap_or(true)).await?,
        "browser_close_tab" => mcp_close_tab(false, args.get("page_id").and_then(Value::as_u64).ok_or_else(||anyhow!("Missing page_id"))? as u32).await?,
        "browser_extract_structure" => mcp_extract_structure(false,args.get("max_items").and_then(Value::as_u64).unwrap_or(100) as usize).await?,
        "browser_network_recent" => mcp_network_recent(false,args.get("limit").and_then(Value::as_u64).unwrap_or(60) as usize,args.get("failures_only").and_then(Value::as_bool).unwrap_or(false),args.get("preserve").and_then(Value::as_bool).unwrap_or(false)).await?,
        "browser_network_request" => mcp_network_request(false,args.get("request_id").and_then(Value::as_u64).ok_or_else(||anyhow!("Missing request_id"))?).await?,
        "browser_console_messages" => mcp_console_messages(false,args.get("limit").and_then(Value::as_u64).unwrap_or(60) as usize,args.get("all_types").and_then(Value::as_bool).unwrap_or(false),args.get("preserve").and_then(Value::as_bool).unwrap_or(false)).await?,
        "browser_diagnostics" => browser_diagnostics(args.get("limit").and_then(Value::as_u64).unwrap_or(40) as usize).await?,
        "browser_memory_current" => browser_memory_current(args.get("limit").and_then(Value::as_u64).unwrap_or(30) as usize).await?,
        "browser_takeover" => browser_takeover(required_str(args,"reason")?).await?,
        "browser_takeover_resume" => browser_takeover_resume()?,
        "browser_takeover_status" => browser_takeover_status()?,
        "headless_browser_tabs" => mcp_tabs(true).await?,
        "headless_browser_open_url" => headless_browser_open_url(required_str(args,"url")?).await?,
        "headless_browser_new_tab" => mcp_new_tab(true,required_str(args,"url")?,args.get("background").and_then(Value::as_bool).unwrap_or(false)).await?,
        "headless_browser_select_tab" => mcp_select_tab(true,args.get("page_id").and_then(Value::as_u64).ok_or_else(||anyhow!("Missing page_id"))? as u32,false).await?,
        "headless_browser_close_tab" => mcp_close_tab(true,args.get("page_id").and_then(Value::as_u64).ok_or_else(||anyhow!("Missing page_id"))? as u32).await?,
        "headless_browser_elements" => headless_browser_elements().await?,
        "headless_browser_click_text" => headless_browser_click_text(required_str(args,"text")?,args.get("exact").and_then(Value::as_bool).unwrap_or(false)).await?,
        "headless_browser_fill_by_label" => headless_browser_fill_by_label(required_str(args,"label")?,required_str(args,"text")?).await?,
        "headless_browser_extract_structure" => mcp_extract_structure(true,args.get("max_items").and_then(Value::as_u64).unwrap_or(100) as usize).await?,
        "headless_browser_network_recent" => mcp_network_recent(true,args.get("limit").and_then(Value::as_u64).unwrap_or(60) as usize,args.get("failures_only").and_then(Value::as_bool).unwrap_or(false),false).await?,
        "headless_browser_network_request" => mcp_network_request(true,args.get("request_id").and_then(Value::as_u64).ok_or_else(||anyhow!("Missing request_id"))?).await?,
        "headless_browser_console_messages" => mcp_console_messages(true,args.get("limit").and_then(Value::as_u64).unwrap_or(60) as usize,args.get("all_types").and_then(Value::as_bool).unwrap_or(false),false).await?,
        "headless_browser_observe" => headless_browser_observe().await?,
        "headless_browser_stop" => headless_browser_stop().await?,
        "desktop_doctor" => serde_json::to_string_pretty(&desktop::doctor().await?)?,
        "desktop_repair_accessibility" => serde_json::to_string_pretty(&desktop::repair_accessibility().await?)?,
        "desktop_apps" => serde_json::to_string_pretty(&desktop::apps(args.get("query").and_then(Value::as_str), args.get("limit").and_then(Value::as_u64).unwrap_or(120) as usize).await?)?,
        "desktop_windows" => serde_json::to_string_pretty(&desktop::windows().await?)?,
        "desktop_capabilities" => serde_json::to_string_pretty(&desktop::capabilities(required_str(args,"window")?).await?)?,
        "desktop_elements" => serde_json::to_string_pretty(&desktop::elements(required_str(args, "window")?).await?)?,
        "desktop_find" => serde_json::to_string_pretty(&desktop::find(required_str(args,"window")?, required_str(args,"query")?, args.get("role").and_then(Value::as_str)).await?)?,
        "desktop_activate_named" => serde_json::to_string_pretty(&desktop::activate_named(required_str(args,"window")?, required_str(args,"query")?, args.get("role").and_then(Value::as_str), args.get("occurrence").and_then(Value::as_u64).unwrap_or(0) as usize).await?)?,
        "desktop_get_text" => serde_json::to_string_pretty(&desktop::get_text(required_str(args,"window")?, required_str(args,"path")?).await?)?,
        "desktop_wait_for" => serde_json::to_string_pretty(&desktop::wait_for(required_str(args,"window")?, required_str(args,"query")?, args.get("role").and_then(Value::as_str), args.get("timeout_seconds").and_then(Value::as_u64).unwrap_or(8)).await?)?,
        "desktop_launch_app" => serde_json::to_string_pretty(&desktop::launch_app(required_str(args,"app")?).await?)?,
        "desktop_fill_credential" => desktop_fill_credential(required_str(args,"window")?, required_str(args,"path")?, required_str(args,"credential_id")?).await?,

        "desktop_activate" => serde_json::to_string_pretty(&desktop::activate(required_str(args, "window")?, required_str(args, "path")?).await?)?,
        "desktop_set_text" => serde_json::to_string_pretty(&desktop::set_text(required_str(args, "window")?, required_str(args, "path")?, required_str(args, "text")?).await?)?,
        "desktop_focus_window" => serde_json::to_string_pretty(&desktop::focus_window(required_str(args, "window")?).await?)?,
        "desktop_key" => serde_json::to_string_pretty(&desktop::key(required_str(args, "window")?, required_str(args, "keys")?).await?)?,
        "desktop_type" => serde_json::to_string_pretty(&desktop::type_text(required_str(args, "window")?, required_str(args, "text")?).await?)?,
        "desktop_observe" => serde_json::to_string_pretty(&desktop::observe(args.get("window").and_then(Value::as_str)).await?)?,
        "desktop_visual_action" => serde_json::to_string_pretty(&desktop::visual_action(
            required_str(args,"window")?, required_str(args,"action")?,
            args.get("x").and_then(Value::as_i64).ok_or_else(||anyhow!("Missing x"))?,
            args.get("y").and_then(Value::as_i64).ok_or_else(||anyhow!("Missing y"))?,
            args.get("end_x").and_then(Value::as_i64), args.get("end_y").and_then(Value::as_i64),
            args.get("direction").and_then(Value::as_str), args.get("amount").and_then(Value::as_u64).unwrap_or(3),
            required_str(args,"purpose")?
        ).await?)?,
        "desktop_playbook" => serde_json::to_string_pretty(&app_playbooks::lookup(required_str(args,"app")?))?,
        "task_create" => { let steps=args.get("steps").and_then(Value::as_array).map(|a|a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default(); serde_json::to_string_pretty(&tasks::create(required_str(args,"title")?, required_str(args,"objective")?, steps)?)? },
        "task_update" => serde_json::to_string_pretty(&tasks::update(required_str(args,"id")?, args.get("status").and_then(Value::as_str), args.get("current_step").and_then(Value::as_str), args.get("completed_step").and_then(Value::as_str), args.get("note").and_then(Value::as_str), args.get("artifact").and_then(Value::as_str))?)?,
        "task_list" => serde_json::to_string_pretty(&tasks::list(args.get("include_completed").and_then(Value::as_bool).unwrap_or(false))?)?,
        "task_get" => serde_json::to_string_pretty(&tasks::get(required_str(args,"id")?)?)?,
        "task_set_phase" => serde_json::to_string_pretty(&tasks::set_phase(required_str(args,"id")?,required_str(args,"phase")?)?)?,
        "task_checkpoint" => serde_json::to_string_pretty(&tasks::checkpoint(required_str(args,"id")?,required_str(args,"label")?,args.get("evidence").and_then(Value::as_str),args.get("kind").and_then(Value::as_str))?)?,
        "task_record_error" => serde_json::to_string_pretty(&tasks::record_error(required_str(args,"id")?,required_str(args,"error")?)?)?,
        "task_resume" => serde_json::to_string_pretty(&tasks::resume(required_str(args,"id")?)?)?,
        "project_inspect" => serde_json::to_string_pretty(&projects::inspect(required_str(args,"path")?)?)?,
        "project_remember" => serde_json::to_string_pretty(&projects::remember(required_str(args,"path")?,args.get("label").and_then(Value::as_str))?)?,
        "project_list" => serde_json::to_string_pretty(&projects::list()?)?,
        "project_forget" => serde_json::to_string_pretty(&projects::forget(required_str(args,"path")?)?)?,
        "terminal_session_create" => serde_json::to_string_pretty(&terminal_sessions::create(required_str(args,"label")?,args.get("cwd").and_then(Value::as_str))?)?,
        "terminal_session_list" => serde_json::to_string_pretty(&terminal_sessions::list()?)?,
        "terminal_session_set_cwd" => serde_json::to_string_pretty(&terminal_sessions::set_cwd(required_str(args,"id")?,required_str(args,"cwd")?)?)?,
        "terminal_session_exec" => {validate_shell_command(required_str(args,"command")?)?;serde_json::to_string_pretty(&terminal_sessions::exec(required_str(args,"id")?,required_str(args,"command")?,args.get("timeout_seconds").and_then(Value::as_u64).unwrap_or(180)).await?)?},
        "terminal_session_history" => serde_json::to_string_pretty(&terminal_sessions::history(required_str(args,"id")?,args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize)?)?,
        "terminal_session_close" => serde_json::to_string_pretty(&terminal_sessions::close(required_str(args,"id")?)?)?,
        "fatir_v1_status" => serde_json::to_string_pretty(&orchestrator::status())?,
        "background_job_start" => { validate_shell_command(required_str(args,"command")?)?; serde_json::to_string_pretty(&jobs::start(required_str(args,"label")?, required_str(args,"command")?, args.get("cwd").and_then(Value::as_str))?)? },
        "background_job_list" => serde_json::to_string_pretty(&jobs::list()?)?,
        "background_job_log" => jobs::log_tail(required_str(args,"id")?, args.get("lines").and_then(Value::as_u64).unwrap_or(120) as usize)?,
        "background_job_cancel" => serde_json::to_string_pretty(&jobs::cancel(required_str(args,"id")?)?)?,
        "scheduled_job_create" => {validate_shell_command(required_str(args,"command")?)?;serde_json::to_string_pretty(&schedules::create(required_str(args,"label")?,required_str(args,"command")?,args.get("cwd").and_then(Value::as_str),required_str(args,"trigger_kind")?,required_str(args,"trigger")?)?)?},
        "scheduled_job_list" => serde_json::to_string_pretty(&schedules::list()?)?,
        "scheduled_job_cancel" => serde_json::to_string_pretty(&schedules::cancel(required_str(args,"id")?)?)?,
        "agent_schedule_create" => {
            let credentials=args.get("credential_ids").and_then(Value::as_array).map(|a|a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
            serde_json::to_string_pretty(&agent_schedules::create(required_str(args,"label")?,required_str(args,"prompt")?,required_str(args,"trigger_kind")?,required_str(args,"trigger")?,required_str(args,"browser_mode")?,credentials)?)?
        },
        "agent_schedule_list" => serde_json::to_string_pretty(&agent_schedules::list()?)?,
        "agent_schedule_cancel" => serde_json::to_string_pretty(&agent_schedules::cancel(required_str(args,"id")?)?)?,
        "checkpoint_files" => { let paths=args.get("paths").and_then(Value::as_array).ok_or_else(||anyhow!("Missing paths"))?.iter().filter_map(Value::as_str).map(|x|expand_path(x).display().to_string()).collect::<Vec<_>>(); serde_json::to_string_pretty(&rollback::checkpoint(&paths, required_str(args,"label")?)?)? },
        "rollback_list" => serde_json::to_string_pretty(&rollback::list(args.get("limit").and_then(Value::as_u64).unwrap_or(30) as usize)?)?,
        "rollback_execute" => serde_json::to_string_pretty(&rollback::execute(required_str(args,"id")?)?)?,
        "software_inventory" => serde_json::to_string_pretty(&software::inventory(args.get("query").and_then(Value::as_str))?)?,
        "software_updates" => serde_json::to_string_pretty(&software::updates()?)?,
        "credential_list" => serde_json::to_string_pretty(&credentials::list()?)?,
        "run_shell_with_credentials" => run_shell_with_credentials(required_str(args,"command")?, args.get("cwd").and_then(Value::as_str), args.get("credentials").and_then(Value::as_object).ok_or_else(||anyhow!("Missing credentials map"))?, args.get("timeout_seconds").and_then(Value::as_u64).unwrap_or(180)).await?,
        "browser_fill_credential" => browser_fill_credential(required_str(args,"element_id")?, required_str(args,"credential_id")?).await?,
        "browser_fill_credential_by_label" => browser_fill_credential_by_label(required_str(args,"label")?, required_str(args,"credential_id")?).await?,
        "recent_actions" => serde_json::to_string_pretty(&memory::recent_actions(args.get("limit").and_then(Value::as_u64).unwrap_or(30) as usize)?)?,
        "recent_activity" => serde_json::to_string_pretty(&memory::recent_activity(args.get("limit").and_then(Value::as_u64).unwrap_or(40) as usize)?)?,
        "routine_summary" => serde_json::to_string_pretty(&memory::routine_summary(700)?)?,
        "routine_create" => { let steps=args.get("steps").and_then(Value::as_array).ok_or_else(||anyhow!("Missing steps"))?.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>(); serde_json::to_string_pretty(&routines::create(required_str(args,"name")?, args.get("description").and_then(Value::as_str).unwrap_or(""), steps)?)? },
        "routine_capture_recent" => serde_json::to_string_pretty(&routines::capture_from_recent(required_str(args,"name")?, args.get("description").and_then(Value::as_str).unwrap_or(""), args.get("action_count").and_then(Value::as_u64).unwrap_or(12) as usize)?)?,
        "routine_list_saved" => serde_json::to_string_pretty(&routines::list()?)?,
        "routine_prepare_run" => serde_json::to_string_pretty(&routines::prepare_run(required_str(args,"id")?)?)?,
        "routine_remove" => serde_json::to_string_pretty(&routines::remove(required_str(args,"id")?)?)?,
        "teach_start" => serde_json::to_string_pretty(&teach::start(required_str(args,"name")?,args.get("description").and_then(Value::as_str).unwrap_or(""))?)?,
        "teach_status" => serde_json::to_string_pretty(&teach::status())?,
        "teach_stop" => serde_json::to_string_pretty(&teach::stop_and_save()?)?,
        "teach_cancel" => serde_json::to_string_pretty(&teach::cancel()?)?,
        "proactive_events" => serde_json::to_string_pretty(&proactive::events(args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize)?)?,
        "proactive_ack" => serde_json::to_string_pretty(&proactive::acknowledge(required_str(args,"id")?)?)?,

        "run_shell_command" => run_shell_command(required_str(args, "command")?, args.get("cwd").and_then(Value::as_str), args.get("timeout_seconds").and_then(Value::as_u64).unwrap_or(180)).await?,
        "run_privileged_command" => run_privileged_command(required_str(args, "command")?, args.get("timeout_seconds").and_then(Value::as_u64).unwrap_or(300)).await?,
        "web_search" => web_search(required_str(args, "query")?, api_key).await?,
        "download_file" => download_file(required_str(args, "url")?, required_str(args, "filename")?).await?,
        "install_apt" => install_apt(required_str(args, "package")?).await?,
        "install_flatpak" => install_flatpak(required_str(args, "app_id")?).await?,
        "install_deb" => install_deb(required_str(args, "path")?).await?,
        "extract_archive" => extract_archive(required_str(args, "archive")?, required_str(args, "destination")?).await?,
        "create_desktop_entry" => create_desktop_entry(required_str(args, "name")?, args.get("exec").and_then(Value::as_str).unwrap_or(""), args.get("icon").and_then(Value::as_str)).await?,
        "move_to_trash" => move_to_trash(required_str(args, "path")?).await?,
        _ => return Err(anyhow!("Unknown tool: {tool}")),
    };
    let _ = teach::record(tool,args,&result,"done");
    let resources = resources_for(tool, args, &result);
    Ok((result.clone(), TraceItem { title: tool.replace('_', " "), detail: shorten(&result, 180), status: "done".into(), resources }))
}


fn resources_for(tool: &str, args: &Value, result: &str) -> Vec<ResourceRef> {
    let mut out = Vec::new();
    let mut push = |path: String, label: String, kind: String| {
        if path.starts_with('/') || path.starts_with("~/") || path.starts_with("http://") || path.starts_with("https://") {
            if !out.iter().any(|r: &ResourceRef| r.path == path) {
                out.push(ResourceRef { label, path, kind });
            }
        }
    };

    match tool {
        "cleanup_scan" => {
            if let Ok(v)=serde_json::from_str::<Value>(result){ if let Some(items)=v.get("candidates").and_then(Value::as_array){ for item in items.iter().take(40){ if let Some(path)=item.get("path").and_then(Value::as_str){ let size=item.get("human").and_then(Value::as_str).unwrap_or(""); let reason=item.get("confidence").and_then(Value::as_str).unwrap_or("review"); let name=Path::new(path).file_name().and_then(|x|x.to_str()).unwrap_or(path); push(path.to_string(),format!("{name} · {size} · {reason}"),"file".into()); } } } }
        }
        "duplicate_scan" => {
            if let Ok(v)=serde_json::from_str::<Value>(result){ if let Some(groups)=v.get("groups").and_then(Value::as_array){ for g in groups.iter().take(20){ if let Some(paths)=g.get("paths").and_then(Value::as_array){ for p in paths.iter().filter_map(Value::as_str).take(8){ let name=Path::new(p).file_name().and_then(|x|x.to_str()).unwrap_or(p); push(p.to_string(),name.to_string(),"file".into()); } } } } }
        }
        "trash_inventory" => {
            if let Ok(v)=serde_json::from_str::<Value>(result){ if let Some(items)=v.get("items").and_then(Value::as_array){ for item in items.iter().take(40){ if let Some(path)=item.get("path").and_then(Value::as_str){ let label=item.get("name").and_then(Value::as_str).unwrap_or(path); push(path.to_string(),label.to_string(),"file".into()); } } } }
        }
        "list_largest_files" => {
            if let Ok(items) = serde_json::from_str::<Vec<Value>>(result) {
                for item in items.into_iter().take(30) {
                    if let Some(path) = item.get("path").and_then(Value::as_str) {
                        let human = item.get("human").and_then(Value::as_str).unwrap_or("");
                        let name = Path::new(path).file_name().and_then(|x| x.to_str()).unwrap_or(path);
                        push(path.to_string(), if human.is_empty(){name.to_string()}else{format!("{name} · {human}")}, "file".into());
                    }
                }
            }
        }
        "list_largest_directories" => {
            if let Ok(items) = serde_json::from_str::<Vec<Value>>(result) {
                for item in items.into_iter().take(30) {
                    if let Some(path) = item.get("path").and_then(Value::as_str) {
                        let human = item.get("human").and_then(Value::as_str).unwrap_or("");
                        let name = Path::new(path).file_name().and_then(|x| x.to_str()).unwrap_or(path);
                        push(path.to_string(), if human.is_empty(){name.to_string()}else{format!("{name} · {human}")}, "folder".into());
                    }
                }
            }
        }
        "list_directory" => {
            if let Ok(items) = serde_json::from_str::<Vec<Value>>(result) {
                for item in items.into_iter().take(60) {
                    if let Some(path) = item.get("path").and_then(Value::as_str) {
                        let label = item.get("name").and_then(Value::as_str).unwrap_or(path).to_string();
                        let kind = if item.get("directory").and_then(Value::as_bool).unwrap_or(false) { "folder" } else { "file" };
                        push(path.to_string(), label, kind.into());
                    }
                }
            }
        }
        "read_file" | "render_pdf_pages" | "open_path" | "install_deb" | "move_to_trash" | "write_text_file" | "create_directory" | "browser_upload_file" => {
            if let Some(path) = args.get("path").and_then(Value::as_str) {
                let label = Path::new(path).file_name().and_then(|x| x.to_str()).unwrap_or(path).to_string();
                push(path.to_string(), label, if Path::new(path).is_dir(){"folder".into()}else{"file".into()});
            }
        }
        "share_latest_screenshot" => {
            if let Ok(v)=serde_json::from_str::<Value>(result){ if let Some(path)=v.get("path").and_then(Value::as_str){ let label=v.get("name").and_then(Value::as_str).unwrap_or("Latest screenshot"); push(path.to_string(),label.to_string(),"file".into()); } }
        }
        "share_whatsapp_send" | "share_email_draft" => {
            if let Some(paths)=args.get("paths").and_then(Value::as_array){ for path in paths.iter().filter_map(Value::as_str).take(25){ let label=Path::new(path).file_name().and_then(|x|x.to_str()).unwrap_or(path); push(path.to_string(),label.to_string(),"file".into()); } }
        }
        "copy_file" | "move_path" => {
            if let Some(path) = args.get("destination").and_then(Value::as_str) {
                let label = Path::new(path).file_name().and_then(|x| x.to_str()).unwrap_or(path).to_string();
                push(path.to_string(), label, if Path::new(path).is_dir(){"folder".into()}else{"file".into()});
            }
        }
        "extract_archive" => {
            if let Some(path) = args.get("destination").and_then(Value::as_str) {
                push(path.to_string(), "Open extracted folder".into(), "folder".into());
            }
        }
        "amazon_keyword_research" => {
            if let Ok(v)=serde_json::from_str::<Value>(result){
                if let Some(path)=v.get("csv_path").and_then(Value::as_str){
                    let label=Path::new(path).file_name().and_then(|x|x.to_str()).unwrap_or("Amazon research CSV");
                    push(path.to_string(),label.to_string(),"file".into());
                }
            }
        }
        "download_file" => {
            if let Some(pos) = result.rfind(" to ") {
                let path = result[pos+4..].trim().to_string();
                let label = Path::new(&path).file_name().and_then(|x| x.to_str()).unwrap_or(&path).to_string();
                push(path, label, "file".into());
            }
        }
        "run_shell_command" | "run_privileged_command" => {
            for raw in result.lines().take(180) {
                let candidate = raw.trim().trim_matches(|c: char| matches!(c, '`' | '\'' | '"' | '[' | ']' | '(' | ')' | ',' | ';' | ':'));
                if candidate.starts_with('/') || candidate.starts_with("~/") {
                    let expanded = expand_path(candidate);
                    if expanded.exists() {
                        let label = expanded.file_name().and_then(|x| x.to_str()).unwrap_or(candidate).to_string();
                        let kind = if expanded.is_dir() { "folder" } else { "file" };
                        push(candidate.to_string(), label, kind.into());
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn required_str<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key).and_then(Value::as_str).ok_or_else(|| anyhow!("Missing {key}"))
}

fn expand_path(input: &str) -> PathBuf {
    if input == "~" { return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")); }
    if let Some(rest) = input.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")).join(rest);
    }
    PathBuf::from(input)
}

pub async fn system_snapshot() -> Result<SystemSnapshot> {
    let mut sys = System::new_all();
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    sys.refresh_all();
    let cpu = sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown CPU".into());
    let cpu_usage = if sys.cpus().is_empty() { 0.0 } else { sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32 };
    let host = System::host_name().unwrap_or_else(|| "Linux".into());
    let os = format!("{} {}", System::name().unwrap_or_default(), System::os_version().unwrap_or_default()).trim().to_string();
    let kernel = System::kernel_version().unwrap_or_default();
    let output = Command::new("df").args(["-h", "/"]).output().await?;
    let df = String::from_utf8_lossy(&output.stdout);
    let cols: Vec<&str> = df.lines().nth(1).unwrap_or("").split_whitespace().collect();
    Ok(SystemSnapshot {
        hostname: host,
        os,
        kernel,
        cpu,
        cpu_usage_percent: cpu_usage,
        memory_used_bytes: sys.used_memory(),
        memory_total_bytes: sys.total_memory(),
        root_used: cols.get(2).unwrap_or(&"").to_string(),
        root_available: cols.get(3).unwrap_or(&"").to_string(),
    })
}

fn list_directory(path: &str) -> Result<String> {
    let p = expand_path(path);
    let mut rows = Vec::new();
    for entry in fs::read_dir(&p).with_context(|| format!("Cannot read {}", p.display()))?.take(300) {
        let e = entry?;
        let meta = e.metadata()?;
        rows.push(json!({"name": e.file_name().to_string_lossy(), "path": e.path(), "directory": meta.is_dir(), "bytes": if meta.is_file(){meta.len()}else{0}}));
    }
    Ok(serde_json::to_string_pretty(&rows)?)
}

async fn largest_directories(path: &str, limit: usize) -> Result<String> {
    let p = expand_path(path);
    let canonical = p.canonicalize().with_context(|| format!("Cannot access {}", p.display()))?;
    if !canonical.is_dir() { return Err(anyhow!("{} is not a directory", canonical.display())); }
    let out = Command::new("du")
        .args(["-B1", "--max-depth=1"])
        .arg(&canonical)
        .output().await.context("du is required")?;
    if !out.status.success() {
        return Err(anyhow!("du failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let root = canonical.to_string_lossy().to_string();
    let mut rows: Vec<(u64, String)> = String::from_utf8_lossy(&out.stdout).lines().filter_map(|line| {
        let (size, path) = line.split_once('\t')?;
        let bytes = size.trim().parse::<u64>().ok()?;
        let path = path.trim().to_string();
        if path == root { None } else { Some((bytes, path)) }
    }).collect();
    rows.sort_by_key(|(bytes, _)| Reverse(*bytes));
    rows.truncate(limit.clamp(1, 100));
    let values: Vec<Value> = rows.into_iter().map(|(bytes, path)| json!({"path":path,"bytes":bytes,"human":human_size(bytes)})).collect();
    Ok(serde_json::to_string_pretty(&values)?)
}

fn largest_files(path: &str, min_mb: u64, limit: usize) -> Result<Vec<LargestFile>> {
    let p = expand_path(path);
    let min = min_mb * 1024 * 1024;
    let mut files: Vec<(u64, PathBuf)> = Vec::new();
    for entry in WalkDir::new(&p).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() { continue; }
        if let Ok(meta) = entry.metadata() {
            if meta.len() >= min { files.push((meta.len(), entry.path().to_path_buf())); }
        }
    }
    files.sort_by_key(|(size, _)| Reverse(*size));
    files.truncate(limit);
    Ok(files.into_iter().map(|(bytes, path)| LargestFile {
        category: classify_path(&path),
        path: path.display().to_string(),
        bytes,
        human: human_size(bytes),
    }).collect())
}

fn classify_path(path: &Path) -> String {
    let s = path.to_string_lossy().to_lowercase();
    if s.contains("/.cache/") || s.contains("/cache/") || s.ends_with(".iso") || s.ends_with(".deb") || s.ends_with(".tar.gz") || s.ends_with(".zip") { "review-cleanup".into() }
    else if s.contains("/downloads/") { "review-download".into() }
    else if s.contains("/documents/") || s.contains("/pictures/") || s.contains("/projects/") { "personal".into() }
    else { "review".into() }
}

async fn read_file(path: &str) -> Result<String> {
    let p = expand_path(path);
    if !p.exists() { return Err(anyhow!("File does not exist: {}", p.display())); }
    let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    if ext == "pdf" {
        let out = Command::new("pdftotext").arg("-layout").arg(&p).arg("-").output().await.context("pdftotext is required (poppler-utils)")?;
        return Ok(shorten(&String::from_utf8_lossy(&out.stdout), 120_000));
    }
    if ext == "docx" {
        let out = Command::new("unzip").args(["-p", p.to_string_lossy().as_ref(), "word/document.xml"]).output().await.context("unzip is required")?;
        let raw = String::from_utf8_lossy(&out.stdout);
        let re = Regex::new(r"<[^>]+>")?;
        let clean = re.replace_all(&raw, " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">");
        return Ok(shorten(&clean, 120_000));
    }
    let data = fs::read(&p)?;
    if data.len() > 2_000_000 { return Ok(format!("File is {} and too large for direct text preview. Read the relevant part or summarize metadata instead.", human_size(data.len() as u64))); }
    match String::from_utf8(data) {
        Ok(s) => Ok(s),
        Err(_) => Ok(format!("Binary file: {} ({})", p.display(), human_size(fs::metadata(&p)?.len()))),
    }
}


async fn render_pdf_pages(path: &str, start_page: usize, end_page: usize) -> Result<String> {
    let p = expand_path(path).canonicalize().with_context(|| format!("Cannot open PDF {path}"))?;
    if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("pdf")) != Some(true) {
        return Err(anyhow!("render_pdf_pages only accepts PDF files"));
    }
    if start_page == 0 || end_page < start_page { return Err(anyhow!("Invalid PDF page range")); }
    if end_page - start_page + 1 > 12 { return Err(anyhow!("Render at most 12 PDF pages per call")); }

    let info = Command::new("pdfinfo").arg(&p).output().await.context("pdfinfo is required (poppler-utils)")?;
    if !info.status.success() { return Err(anyhow!("Could not inspect PDF metadata")); }
    let info_text = String::from_utf8_lossy(&info.stdout);
    let page_count = info_text.lines()
        .find_map(|line| line.strip_prefix("Pages:").and_then(|x| x.trim().parse::<usize>().ok()))
        .unwrap_or(end_page);
    if start_page > page_count { return Err(anyhow!("PDF has only {page_count} page(s)")); }
    let end_page = end_page.min(page_count);

    let base = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir").join("pdf-renders").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&base)?;
    let prefix = base.join("page");

    let start_arg = start_page.to_string();
    let end_arg = end_page.to_string();
    let status = Command::new("pdftoppm")
        .args(["-png", "-r", "135", "-f", start_arg.as_str(), "-l", end_arg.as_str()])
        .arg(&p)
        .arg(&prefix)
        .status().await.context("pdftoppm is required (poppler-utils)")?;
    if !status.success() { return Err(anyhow!("PDF page rendering failed")); }

    let mut images: Vec<PathBuf> = fs::read_dir(&base)?
        .filter_map(|e| e.ok().map(|x| x.path()))
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("png"))
        .collect();
    images.sort();
    if images.is_empty() { return Err(anyhow!("PDF renderer produced no page images")); }

    Ok(serde_json::to_string_pretty(&json!({
        "path": p.display().to_string(),
        "page_count": page_count,
        "start_page": start_page,
        "end_page": end_page,
        "images": images.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()
    }))?)
}

async fn open_path(path: &str) -> Result<String> {
    if path.starts_with("http://") || path.starts_with("https://") {
        return Err(anyhow!("open_path is for local files/folders. Use browser_open_url for websites."));
    }
    let target = expand_path(path).to_string_lossy().to_string();
    Command::new("xdg-open").arg(&target).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    Ok(format!("Opened {target}"))
}

async fn take_screenshot() -> Result<String> {
    let dir = dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures")).join("screenshot");
    fs::create_dir_all(&dir)?;
    let file = dir.join(format!("ksnip_{}.png", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let status = if which("gnome-screenshot") {
        Command::new("gnome-screenshot").args(["-f", file.to_string_lossy().as_ref()]).status().await?
    } else if which("scrot") {
        Command::new("scrot").arg(file.to_string_lossy().as_ref()).status().await?
    } else if which("flameshot") {
        Command::new("flameshot").args(["full", "-p", file.to_string_lossy().as_ref()]).status().await?
    } else { return Err(anyhow!("Install gnome-screenshot, scrot, or flameshot to capture screenshots")); };
    if !status.success() { return Err(anyhow!("Screenshot command failed")); }
    // A screenshot is immediately useful as a share object. Copy the actual bitmap
    // to X11 clipboard while still returning the canonical file path.
    let _ = share::copy(&[file.display().to_string()]);
    Ok(file.display().to_string())
}

tokio::task_local! {
    // 0 = blocked, 1 = Fatir-managed browser, 2 = user's already-running active Chrome session.
    static BROWSER_EXECUTION_MODE: u8;
}

pub async fn with_browser_execution_context<F>(allowed: bool, active_session: bool, future: F) -> F::Output
where
    F: std::future::Future,
{
    let mode = if !allowed { 0 } else if active_session { 2 } else { 1 };
    BROWSER_EXECUTION_MODE.scope(mode, future).await
}

pub async fn with_browser_execution_allowed<F>(allowed: bool, future: F) -> F::Output
where
    F: std::future::Future,
{
    with_browser_execution_context(allowed, false, future).await
}

fn browser_execution_mode() -> u8 {
    BROWSER_EXECUTION_MODE.try_with(|mode| *mode).unwrap_or(0)
}

fn browser_execution_allowed() -> bool { browser_execution_mode() != 0 }
fn active_browser_session_requested() -> bool { browser_execution_mode() == 2 }

const CDP_PORT: u16 = 9223;
const ACTIVE_MCP_SESSION: &str = "fatir-active-browser";

fn browser_binary() -> Option<&'static str> {
    ["google-chrome-stable", "google-chrome", "chromium", "chromium-browser", "brave-browser", "microsoft-edge", "vivaldi-stable"]
        .into_iter().find(|name| which(name))
}

async fn browser_debug_ready() -> bool {
    Client::new()
        .get(format!("http://127.0.0.1:{CDP_PORT}/json/version"))
        .timeout(std::time::Duration::from_millis(450))
        .send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

async fn active_chrome_running() -> bool {
    // Do not start anything here. Active-session mode means the user owns the browser process.
    let output = Command::new("sh").arg("-lc").arg(
        r"wmctrl -lx 2>/dev/null | grep -Eqi 'google-chrome|chromium|chrome\.google-chrome|brave|microsoft-edge|vivaldi'"
    ).status().await;
    output.map(|s|s.success()).unwrap_or(false)
}

async fn ensure_fatir_browser() -> Result<()> {
    if !browser_execution_allowed() {
        return Err(anyhow!("Browser startup blocked: the current turn does not have explicit browser intent"));
    }
    if active_browser_session_requested() {
        if active_chrome_running().await { return Ok(()); }
        return Err(anyhow!("Active browser session requested, but no running Chrome-family browser was found. Fatir will not launch its own browser for an active-session request."));
    }
    if browser_debug_ready().await { return Ok(()); }
    let bin = browser_binary().ok_or_else(|| anyhow!("Fatir browser control needs Chrome, Chromium, Brave, Edge or Vivaldi installed"))?;
    let profile = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir").join("browser-profile");
    fs::create_dir_all(&profile)?;

    Command::new(bin)
        .arg(format!("--remote-debugging-port={CDP_PORT}"))
        .arg("--remote-debugging-address=127.0.0.1")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("about:blank")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Could not start {bin} for Fatir browser control"))?;

    for _ in 0..45 {
        if browser_debug_ready().await { return Ok(()); }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    Err(anyhow!("Fatir started a controlled browser but could not connect to its DevTools interface"))
}


const CHROME_DEVTOOLS_MCP_VERSION: &str = "1.10.1";
static CHROME_MCP_READY: AtomicBool = AtomicBool::new(false);
static ACTIVE_CHROME_MCP_READY: AtomicBool = AtomicBool::new(false);
static CHROME_MCP_HEADLESS_READY: AtomicBool = AtomicBool::new(false);
const HEADLESS_MCP_SESSION: &str = "fatir-headless";

fn chrome_mcp_bin() -> PathBuf {
    let local = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir").join("chrome-devtools-mcp").join("node_modules").join(".bin").join("chrome-devtools");
    if local.exists() { local } else { PathBuf::from("chrome-devtools") }
}

async fn chrome_mcp_exec_session(session: Option<&str>, args: &[String]) -> Result<String> {
    let bin = chrome_mcp_bin();
    if !bin.exists() && !which("chrome-devtools") {
        return Err(anyhow!("Chrome DevTools MCP CLI is not installed. Re-run Fatir's installer to install chrome-devtools-mcp {CHROME_DEVTOOLS_MCP_VERSION}."));
    }
    let mut command=Command::new(&bin);
    if let Some(id)=session { command.arg("--sessionId").arg(id); }
    let output = command
        .args(args)
        .env("CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output().await
        .with_context(|| format!("Could not run {}", bin.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(anyhow!("Chrome DevTools MCP command failed: {}", if stderr.is_empty(){stdout}else{stderr}));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn chrome_mcp_exec(args: &[String]) -> Result<String> {
    if active_browser_session_requested() { chrome_mcp_exec_session(Some(ACTIVE_MCP_SESSION),args).await }
    else { chrome_mcp_exec_session(None,args).await }
}

async fn ensure_chrome_mcp() -> Result<()> {
    ensure_fatir_browser().await?;
    if active_browser_session_requested() {
        if ACTIVE_CHROME_MCP_READY.load(Ordering::SeqCst) { return Ok(()); }
        let bin = chrome_mcp_bin();
        if !bin.exists() && !which("chrome-devtools") {
            return Err(anyhow!("Chrome DevTools MCP CLI is not installed. Re-run Fatir's installer."));
        }
        // Active-session mode must attach only. `--auto-connect` intentionally does not launch Chrome.
        let _ = Command::new(&bin).arg("--sessionId").arg(ACTIVE_MCP_SESSION).arg("stop")
            .env("CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS","1").output().await;
        let start_args = vec!["start".to_string(), "--auto-connect".to_string()];
        if let Err(err)=chrome_mcp_exec_session(Some(ACTIVE_MCP_SESSION),&start_args).await {
            return Err(anyhow!("Could not attach to the active Chrome session: {}. Fatir did not launch another browser. In your existing Chrome (144+), open chrome://inspect/#remote-debugging, enable remote debugging, allow the connection prompt, then retry.",err));
        }
        let verify=vec!["list_pages".to_string()];
        if let Err(err)=chrome_mcp_exec_session(Some(ACTIVE_MCP_SESSION),&verify).await {
            return Err(anyhow!("Active Chrome is running, but Fatir could not inspect its tabs: {}. Fatir did not launch another browser. Enable remote debugging in that same Chrome at chrome://inspect/#remote-debugging and retry.",err));
        }
        ACTIVE_CHROME_MCP_READY.store(true,Ordering::SeqCst);
        return Ok(());
    }
    if CHROME_MCP_READY.load(Ordering::SeqCst) { return Ok(()); }
    let bin = chrome_mcp_bin();
    if !bin.exists() && !which("chrome-devtools") {
        return Err(anyhow!("Chrome DevTools MCP CLI is not installed. Re-run Fatir's installer."));
    }
    let _ = Command::new(&bin).arg("stop").env("CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS","1").output().await;
    let start_args = vec![
        "start".to_string(),
        format!("--browser-url=http://127.0.0.1:{CDP_PORT}"),
    ];
    chrome_mcp_exec(&start_args).await?;
    let verify = vec!["list_pages".to_string()];
    chrome_mcp_exec(&verify).await.context("Chrome DevTools MCP started but could not inspect Fatir's browser")?;
    CHROME_MCP_READY.store(true, Ordering::SeqCst);
    Ok(())
}

async fn chrome_mcp_call(tool: &str, args: Vec<String>) -> Result<String> {
    ensure_chrome_mcp().await?;
    let mut cmd = Vec::with_capacity(args.len()+1);
    cmd.push(tool.to_string());
    cmd.extend(args.clone());
    match chrome_mcp_exec(&cmd).await {
        Ok(v) => Ok(v),
        Err(first) => {
            if active_browser_session_requested() { ACTIVE_CHROME_MCP_READY.store(false, Ordering::SeqCst); }
            else { CHROME_MCP_READY.store(false, Ordering::SeqCst); }
            ensure_chrome_mcp().await?;
            let mut retry = Vec::with_capacity(args.len()+1);
            retry.push(tool.to_string()); retry.extend(args);
            chrome_mcp_exec(&retry).await.with_context(|| format!("Chrome DevTools MCP failed after reconnect: {first}"))
        }
    }
}

async fn ensure_headless_chrome_mcp() -> Result<()> {
    if !browser_execution_allowed() {
        return Err(anyhow!("Headless browser startup blocked: the current turn does not have explicit browser intent"));
    }
    if CHROME_MCP_HEADLESS_READY.load(Ordering::SeqCst) { return Ok(()); }
    let bin=chrome_mcp_bin();
    if !bin.exists() && !which("chrome-devtools") { return Err(anyhow!("Chrome DevTools MCP CLI is not installed. Re-run Fatir's installer.")); }
    let _=chrome_mcp_exec_session(Some(HEADLESS_MCP_SESSION), &["stop".into()]).await;
    let profile=dirs::data_local_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir").join("headless-browser-profile");
    fs::create_dir_all(&profile)?;
    let start_args=vec![
        "start".into(), "--headless=true".into(), format!("--userDataDir={}",profile.display()), "--viewport=1440x1000".into()
    ];
    chrome_mcp_exec_session(Some(HEADLESS_MCP_SESSION),&start_args).await?;
    chrome_mcp_exec_session(Some(HEADLESS_MCP_SESSION), &["list_pages".into()]).await
        .context("Headless Chrome DevTools MCP started but could not list pages")?;
    CHROME_MCP_HEADLESS_READY.store(true,Ordering::SeqCst); Ok(())
}

async fn headless_mcp_call(tool:&str,args:Vec<String>)->Result<String>{
    ensure_headless_chrome_mcp().await?;
    let mut cmd=Vec::with_capacity(args.len()+1);cmd.push(tool.to_string());cmd.extend(args.clone());
    match chrome_mcp_exec_session(Some(HEADLESS_MCP_SESSION),&cmd).await{
        Ok(v)=>Ok(v),
        Err(first)=>{CHROME_MCP_HEADLESS_READY.store(false,Ordering::SeqCst);ensure_headless_chrome_mcp().await?;let mut retry=vec![tool.to_string()];retry.extend(args);chrome_mcp_exec_session(Some(HEADLESS_MCP_SESSION),&retry).await.with_context(||format!("Headless Chrome DevTools MCP failed after reconnect: {first}"))}
    }
}

fn chrome_mcp_page_id_from_list(pages: &str) -> Option<u32> {
    let selected = Regex::new(r"(?m)^\s*(\d+):\s+.*\[selected\]\s*$").ok()?;
    if let Some(c)=selected.captures(pages) { return c.get(1)?.as_str().parse().ok(); }
    let first = Regex::new(r"(?m)^\s*(\d+):\s+").ok()?;
    first.captures(pages)?.get(1)?.as_str().parse().ok()
}

async fn chrome_mcp_page_id() -> Result<u32> {
    let pages = chrome_mcp_call("list_pages", vec![]).await?;
    chrome_mcp_page_id_from_list(&pages).ok_or_else(|| anyhow!("Chrome DevTools MCP found no active browser page"))
}

async fn chrome_mcp_snapshot() -> Result<(u32,String)> {
    let page = chrome_mcp_page_id().await?;
    let snapshot = chrome_mcp_call("take_snapshot", vec![page.to_string()]).await?;
    Ok((page, snapshot))
}

fn semantic_text_score(label:&str,wanted:&str,exact:bool)->i32{
    let norm=|v:&str|v.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    let l=norm(label); let w=norm(wanted); if l.is_empty()||w.is_empty(){return 0;}
    if l==w{return 220;}
    if exact{return 0;}
    if l.contains(&w){return 150;}
    if w.contains(&l)&&l.len()>2{return 105;}
    let lt:HashSet<&str>=l.split_whitespace().collect(); let wt:HashSet<&str>=w.split_whitespace().collect();
    let common=lt.intersection(&wt).count(); if common==0{return 0;}
    let union=lt.union(&wt).count().max(1);
    let pct=(common*100/union) as i32;
    if pct>=75{110+pct/4}else if pct>=50{75+pct/5}else if pct>=34{48+pct/6}else{0}
}

fn chrome_mcp_uid(snapshot: &str, wanted: &str, exact: bool, preferred_roles: &[&str]) -> Option<(String,String,String)> {
    let wanted = wanted.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    if wanted.is_empty() { return None; }
    let re = Regex::new(r#"uid=([^\s]+)\s+([A-Za-z][A-Za-z0-9_-]*)(?:\s+\"([^\"]*)\")?"#).ok()?;
    let mut best: Option<(i32,String,String,String)> = None;
    for cap in re.captures_iter(snapshot) {
        let uid=cap.get(1)?.as_str().to_string();
        let role=cap.get(2)?.as_str().to_string();
        let label=cap.get(3).map(|m|m.as_str()).unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
        let mut score=semantic_text_score(&label,&wanted,exact); if score==0{continue;}
        if preferred_roles.iter().any(|r| role.eq_ignore_ascii_case(r)) { score+=35; }
        if matches!(role.to_ascii_lowercase().as_str(), "button"|"link"|"textbox"|"searchbox"|"combobox"|"menuitem"|"tab") { score+=10; }
        if best.as_ref().map(|x|score>x.0).unwrap_or(true) { best=Some((score,uid,role,label)); }
    }
    best.map(|(_,u,r,l)|(u,r,l))
}

async fn chrome_mcp_snapshot_for(headless:bool)->Result<(u32,String)>{
    let pages=if headless{headless_mcp_call("list_pages",vec![]).await?}else{chrome_mcp_call("list_pages",vec![]).await?};
    let page=chrome_mcp_page_id_from_list(&pages).ok_or_else(||anyhow!("Chrome DevTools MCP found no active browser page"))?;
    let snapshot=if headless{headless_mcp_call("take_snapshot",vec![page.to_string()]).await?}else{chrome_mcp_call("take_snapshot",vec![page.to_string()]).await?};
    Ok((page,snapshot))
}

async fn chrome_mcp_url_for(headless:bool)->Result<String>{
    let pages=if headless{headless_mcp_call("list_pages",vec![]).await?}else{chrome_mcp_call("list_pages",vec![]).await?};
    let selected=pages.lines().find(|line|line.contains("[selected]")).or_else(||pages.lines().find(|line|Regex::new(r"^\s*\d+:\s+").ok().map(|r|r.is_match(line)).unwrap_or(false)))
        .ok_or_else(||anyhow!("Chrome DevTools MCP found no active page"))?;
    let (_,rest)=selected.split_once(':').ok_or_else(||anyhow!("Could not parse Chrome DevTools MCP page list"))?;
    let url=rest.replace("[selected]","").trim().to_string(); if url.is_empty(){return Err(anyhow!("Chrome DevTools MCP active page URL was empty"));} Ok(url)
}

async fn chrome_mcp_elements() -> Result<String> {
    let (page,snapshot)=chrome_mcp_snapshot().await?;
    let url=chrome_mcp_current_url().await.unwrap_or_default();
    let _=browser_memory::remember_snapshot(&url,page,&snapshot);
    let takeover_recommended=snapshot_takeover_recommended(&snapshot);
    Ok(serde_json::to_string_pretty(&json!({"engine":"chrome-devtools-mcp","page_id":page,"url":url,"snapshot":snapshot,"human_takeover_recommended":takeover_recommended}))?)
}

async fn chrome_mcp_click_text(text: &str, exact: bool) -> Result<String> {
    let url=chrome_mcp_current_url().await.unwrap_or_default();
    let (page,snapshot)=chrome_mcp_snapshot().await?;
    let _=browser_memory::remember_snapshot(&url,page,&snapshot);
    let mut matched=chrome_mcp_uid(&snapshot,text,exact,&["button","link","menuitem","tab"]);
    let mut healed=false;
    if matched.is_none() && !exact {
        for remembered in browser_memory::candidates(&url,"click",text,8) {
            if let Some(v)=chrome_mcp_uid(&snapshot,&remembered.label,false,&[remembered.role.as_str(),"button","link","menuitem","tab"]){matched=Some(v);healed=true;break;}
        }
    }
    let (mut uid,mut role,mut label)=matched.ok_or_else(||anyhow!("Chrome DevTools MCP could not find a control matching '{text}'"))?;
    let first=chrome_mcp_call("click",vec![page.to_string(),uid.clone(),"--includeSnapshot".into(),"true".into()]).await;
    let output=match first{
        Ok(v)=>v,
        Err(first_err)=>{
            // Reactive pages can invalidate an MCP uid between snapshot and click. Re-snapshot
            // and resolve the semantic target once instead of blindly retrying a stale uid.
            let (page2,snapshot2)=chrome_mcp_snapshot().await?;
            let _=browser_memory::remember_snapshot(&url,page2,&snapshot2);
            let Some((uid2,role2,label2))=chrome_mcp_uid(&snapshot2,&label,false,&[role.as_str(),"button","link","menuitem","tab"]) else {let _=browser_memory::record_target(&url,"click",&role,&label,false,Some(text));return Err(first_err);};
            uid=uid2;role=role2;label=label2;healed=true;
            chrome_mcp_call("click",vec![page2.to_string(),uid.clone(),"--includeSnapshot".into(),"true".into()]).await?
        }
    };
    let after_url=chrome_mcp_current_url().await.unwrap_or(url.clone());
    let _=browser_memory::record_target(&url,"click",&role,&label,true,Some(text));
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"engine":"chrome-devtools-mcp","page_id":page,"uid":uid,"role":role,"clicked":label,"self_healed":healed,"url":after_url,"result":output}))?)
}

async fn chrome_mcp_fill_by_label(label: &str, text: &str) -> Result<String> {
    let url=chrome_mcp_current_url().await.unwrap_or_default();
    let (page,snapshot)=chrome_mcp_snapshot().await?;
    let _=browser_memory::remember_snapshot(&url,page,&snapshot);
    let mut matched=chrome_mcp_uid(&snapshot,label,false,&["textbox","searchbox","combobox","spinbutton"]);
    let mut healed=false;
    if matched.is_none(){
        for remembered in browser_memory::candidates(&url,"fill",label,8){
            if let Some(v)=chrome_mcp_uid(&snapshot,&remembered.label,false,&[remembered.role.as_str(),"textbox","searchbox","combobox","spinbutton"]){matched=Some(v);healed=true;break;}
        }
    }
    let (mut uid,mut role,mut found)=matched.ok_or_else(||anyhow!("Chrome DevTools MCP could not find a field matching '{label}'"))?;
    let first=chrome_mcp_call("fill",vec![page.to_string(),uid.clone(),text.to_string(),"--includeSnapshot".into(),"true".into()]).await;
    let output=match first{
        Ok(v)=>v,
        Err(first_err)=>{
            let (page2,snapshot2)=chrome_mcp_snapshot().await?;let _=browser_memory::remember_snapshot(&url,page2,&snapshot2);
            let Some((uid2,role2,label2))=chrome_mcp_uid(&snapshot2,&found,false,&[role.as_str(),"textbox","searchbox","combobox","spinbutton"]) else{let _=browser_memory::record_target(&url,"fill",&role,&found,false,Some(label));return Err(first_err);};
            uid=uid2;role=role2;found=label2;healed=true;
            chrome_mcp_call("fill",vec![page2.to_string(),uid.clone(),text.to_string(),"--includeSnapshot".into(),"true".into()]).await?
        }
    };
    let _=browser_memory::record_target(&url,"fill",&role,&found,true,Some(label));
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"engine":"chrome-devtools-mcp","page_id":page,"uid":uid,"role":role,"field":found,"characters":text.chars().count(),"self_healed":healed,"url":url,"result":output}))?)
}


fn chrome_mcp_label_for_uid(snapshot: &str, target_uid: &str) -> Option<String> {
    let re = Regex::new(r#"uid=([^\s]+)\s+[A-Za-z][A-Za-z0-9_-]*(?:\s+\"([^\"]*)\")?"#).ok()?;
    for cap in re.captures_iter(snapshot) {
        if cap.get(1).map(|m|m.as_str()) == Some(target_uid) {
            return cap.get(2).map(|m|m.as_str().trim().to_string()).filter(|v|!v.is_empty());
        }
    }
    None
}

async fn chrome_mcp_click_uid(uid: &str) -> Result<String> {
    let page=chrome_mcp_page_id().await?;let url=chrome_mcp_current_url().await.unwrap_or_default();let remembered=browser_memory::semantic_for_uid_on_url(uid,&url);
    match chrome_mcp_call("click",vec![page.to_string(),uid.to_string(),"--includeSnapshot".into(),"true".into()]).await{
        Ok(output)=>{if let Some(t)=remembered{let _=browser_memory::record_target(&url,"click",&t.role,&t.label,true,None);}
            Ok(serde_json::to_string_pretty(&json!({"ok":true,"engine":"chrome-devtools-mcp","page_id":page,"uid":uid,"self_healed":false,"result":output}))?)}
        Err(first_err)=>{
            let Some(t)=remembered else{return Err(first_err);};
            let (page2,snapshot)=chrome_mcp_snapshot().await?;let _=browser_memory::remember_snapshot(&url,page2,&snapshot);
            let Some((new_uid,role,label))=chrome_mcp_uid(&snapshot,&t.label,false,&[t.role.as_str(),"button","link","menuitem","tab"]) else{let _=browser_memory::record_target(&url,"click",&t.role,&t.label,false,None);return Err(first_err);};
            let output=chrome_mcp_call("click",vec![page2.to_string(),new_uid.clone(),"--includeSnapshot".into(),"true".into()]).await?;
            let _=browser_memory::record_target(&url,"click",&role,&label,true,Some(&t.label));
            Ok(serde_json::to_string_pretty(&json!({"ok":true,"engine":"chrome-devtools-mcp","page_id":page2,"uid":new_uid,"self_healed":true,"recovered_from_uid":uid,"clicked":label,"result":output}))?)
        }
    }
}

async fn chrome_mcp_fill_uid(uid: &str, text: &str) -> Result<String> {
    let page=chrome_mcp_page_id().await?;let url=chrome_mcp_current_url().await.unwrap_or_default();let remembered=browser_memory::semantic_for_uid_on_url(uid,&url);
    match chrome_mcp_call("fill",vec![page.to_string(),uid.to_string(),text.to_string(),"--includeSnapshot".into(),"true".into()]).await{
        Ok(output)=>{if let Some(t)=remembered{let _=browser_memory::record_target(&url,"fill",&t.role,&t.label,true,None);}Ok(serde_json::to_string_pretty(&json!({"ok":true,"engine":"chrome-devtools-mcp","page_id":page,"uid":uid,"characters":text.chars().count(),"self_healed":false,"result":output}))?)}
        Err(first_err)=>{
            let Some(t)=remembered else{return Err(first_err);};
            let (page2,snapshot)=chrome_mcp_snapshot().await?;let _=browser_memory::remember_snapshot(&url,page2,&snapshot);
            let Some((new_uid,role,label))=chrome_mcp_uid(&snapshot,&t.label,false,&[t.role.as_str(),"textbox","searchbox","combobox","spinbutton"]) else{let _=browser_memory::record_target(&url,"fill",&t.role,&t.label,false,None);return Err(first_err);};
            let output=chrome_mcp_call("fill",vec![page2.to_string(),new_uid.clone(),text.to_string(),"--includeSnapshot".into(),"true".into()]).await?;
            let _=browser_memory::record_target(&url,"fill",&role,&label,true,Some(&t.label));
            Ok(serde_json::to_string_pretty(&json!({"ok":true,"engine":"chrome-devtools-mcp","page_id":page2,"uid":new_uid,"characters":text.chars().count(),"self_healed":true,"recovered_from_uid":uid,"field":label,"result":output}))?)
        }
    }
}


async fn chrome_mcp_current_url() -> Result<String> {
    let pages=chrome_mcp_call("list_pages",vec![]).await?;
    let selected=pages.lines().find(|line|line.contains("[selected]")).or_else(||pages.lines().find(|line|Regex::new(r"^\s*\d+:\s+").ok().map(|r|r.is_match(line)).unwrap_or(false)))
        .ok_or_else(||anyhow!("Chrome DevTools MCP found no active page"))?;
    let (_,rest)=selected.split_once(':').ok_or_else(||anyhow!("Could not parse Chrome DevTools MCP page list"))?;
    let url=rest.replace("[selected]","").trim().to_string();
    if url.is_empty(){return Err(anyhow!("Chrome DevTools MCP active page URL was empty"));}
    Ok(url)
}

async fn chrome_mcp_screenshot() -> Result<String> {
    let page=chrome_mcp_page_id().await?;
    let dir=dirs::picture_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join("Pictures")).join("Fatir").join("Browser");
    fs::create_dir_all(&dir)?;
    let file=dir.join(format!("browser-mcp-{}.png",chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")));
    chrome_mcp_call("take_screenshot",vec![page.to_string(),"--filePath".into(),file.display().to_string()]).await?;
    if !file.is_file(){return Err(anyhow!("Chrome DevTools MCP did not produce the requested screenshot"));}
    let url=chrome_mcp_current_url().await.unwrap_or_default();
    Ok(serde_json::to_string(&json!({
        "screenshot":file.display().to_string(),"url":url,"page_id":page,
        "automation":"Chrome DevTools MCP","semantic_snapshot":true,"physical_mouse_moved":false,
        "coordinate_system":"Use browser_elements/browser_click_element MCP uids for interaction. Coordinate clicks remain last-resort CDP fallback."
    }))?)
}

async fn chrome_mcp_open_url(value: &str) -> Result<String> {
    let page=chrome_mcp_page_id().await?;
    let output=chrome_mcp_call("navigate_page",vec![page.to_string(),"--url".into(),value.to_string(),"--timeout".into(),"10000".into()]).await?;
    Ok(format!("Opened {value} with Chrome DevTools MCP on page {page}. {output}"))
}

async fn chrome_mcp_type(text: &str) -> Result<String> {
    let page=chrome_mcp_page_id().await?;
    chrome_mcp_call("type_text",vec![page.to_string(),text.to_string()]).await
}

fn chrome_mcp_key_name(key: &str) -> String {
    key.split('+').map(|p| match p.to_ascii_lowercase().as_str() {
        "ctrl"|"control" => "Control".to_string(), "alt" => "Alt".to_string(), "shift" => "Shift".to_string(),
        "meta"|"super" => "Meta".to_string(), "return"|"enter" => "Enter".to_string(), "esc"|"escape" => "Escape".to_string(),
        "left" => "ArrowLeft".to_string(), "right" => "ArrowRight".to_string(), "up" => "ArrowUp".to_string(), "down" => "ArrowDown".to_string(),
        _ => p.to_string(),
    }).collect::<Vec<_>>().join("+")
}

async fn chrome_mcp_key(key: &str) -> Result<String> {
    let page=chrome_mcp_page_id().await?;
    chrome_mcp_call("press_key",vec![page.to_string(),chrome_mcp_key_name(key)]).await
}

async fn mcp_tabs(headless:bool)->Result<String>{
    let out=if headless{headless_mcp_call("list_pages",vec![]).await?}else{chrome_mcp_call("list_pages",vec![]).await?};
    Ok(serde_json::to_string_pretty(&json!({"engine":"chrome-devtools-mcp","headless":headless,"pages":out}))?)
}

async fn mcp_new_tab(headless:bool,url:&str,background:bool)->Result<String>{
    let parsed=url::Url::parse(url).map_err(|_|anyhow!("Invalid URL: {url}"))?;
    if !matches!(parsed.scheme(),"http"|"https"){return Err(anyhow!("Only http/https URLs are supported"));}
    let args=vec![url.to_string(),"--background".into(),background.to_string(),"--timeout".into(),"12000".into()];
    let out=if headless{headless_mcp_call("new_page",args).await?}else{chrome_mcp_call("new_page",args).await?};
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"headless":headless,"url":url,"background":background,"result":out}))?)
}

async fn mcp_select_tab(headless:bool,page_id:u32,bring_to_front:bool)->Result<String>{
    let mut args=vec![page_id.to_string()];
    if !headless{args.extend(["--bringToFront".into(),bring_to_front.to_string()]);}
    let out=if headless{headless_mcp_call("select_page",args).await?}else{chrome_mcp_call("select_page",args).await?};
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"headless":headless,"page_id":page_id,"result":out}))?)
}

async fn mcp_close_tab(headless:bool,page_id:u32)->Result<String>{
    let out=if headless{headless_mcp_call("close_page",vec![page_id.to_string()]).await?}else{chrome_mcp_call("close_page",vec![page_id.to_string()]).await?};
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"headless":headless,"closed_page_id":page_id,"result":out}))?)
}

fn structure_script(max_items:usize)->String{
    let raw=r#"() => {
      const MAX=__MAX__;
      const clean=v=>(v||'').replace(/\s+/g,' ').trim();
      const visible=el=>{const r=el.getBoundingClientRect();const s=getComputedStyle(el);return r.width>1&&r.height>1&&s.display!=='none'&&s.visibility!=='hidden';};
      const text=el=>clean(el.innerText||el.textContent||'').slice(0,1200);
      const headings=[...document.querySelectorAll('h1,h2,h3,h4,h5,h6')].filter(visible).slice(0,MAX).map(x=>({level:x.tagName.toLowerCase(),text:text(x)})).filter(x=>x.text);
      const links=[...document.querySelectorAll('a[href]')].filter(visible).slice(0,MAX).map(x=>({text:text(x).slice(0,300),href:x.href})).filter(x=>x.text||x.href);
      const lists=[...document.querySelectorAll('ul,ol')].filter(visible).slice(0,Math.min(MAX,50)).map(x=>({type:x.tagName.toLowerCase(),items:[...x.querySelectorAll(':scope > li')].slice(0,40).map(text).filter(Boolean)})).filter(x=>x.items.length);
      const tables=[...document.querySelectorAll('table')].filter(visible).slice(0,20).map(t=>({caption:text(t.querySelector('caption')||document.createElement('span')),rows:[...t.querySelectorAll('tr')].slice(0,60).map(r=>[...r.querySelectorAll('th,td')].slice(0,30).map(c=>text(c).slice(0,500)))})).filter(x=>x.rows.length);
      const forms=[...document.querySelectorAll('form')].filter(visible).slice(0,30).map(f=>({action:f.action||'',method:(f.method||'get').toLowerCase(),fields:[...f.querySelectorAll('input:not([type=hidden]),textarea,select,button')].filter(visible).slice(0,60).map(el=>({tag:el.tagName.toLowerCase(),type:el.getAttribute('type')||'',name:el.getAttribute('name')||'',label:clean(el.getAttribute('aria-label')||el.getAttribute('placeholder')||el.innerText||el.value||'').slice(0,300)}))}));
      const cardSelectors=['article','[role=listitem]','[data-testid*=product]','[class*=product]','[class*=card]','[class*=result]'];
      const seen=new Set();const cards=[];
      for(const sel of cardSelectors){for(const el of document.querySelectorAll(sel)){if(cards.length>=MAX)break;if(!visible(el))continue;const t=text(el);if(t.length<20||t.length>5000)continue;const key=t.slice(0,240);if(seen.has(key))continue;seen.add(key);const a=el.querySelector('a[href]');cards.push({text:t.slice(0,1600),href:a?a.href:''});}if(cards.length>=MAX)break;}
      return {url:location.href,title:document.title,headings,tables,lists,links,cards,forms};
    }"#;
    raw.replace("__MAX__",&max_items.clamp(10,300).to_string())
}

async fn mcp_extract_structure(headless:bool,max_items:usize)->Result<String>{
    let pages=if headless{headless_mcp_call("list_pages",vec![]).await?}else{chrome_mcp_call("list_pages",vec![]).await?};
    let page=chrome_mcp_page_id_from_list(&pages).ok_or_else(||anyhow!("No active browser page"))?;
    let args=vec![structure_script(max_items),"--pageId".into(),page.to_string()];
    let result=if headless{headless_mcp_call("evaluate_script",args).await?}else{chrome_mcp_call("evaluate_script",args).await?};
    Ok(serde_json::to_string_pretty(&json!({"engine":"chrome-devtools-mcp","headless":headless,"page_id":page,"structured":result}))?)
}

fn redact_sensitive_browser_text(raw:&str)->String{
    let mut out=raw.to_string();
    if let Ok(headers)=Regex::new(r"(?im)^(\s*)(cookie|set-cookie|authorization|proxy-authorization|x-api-key|x-auth-token)(\s*:\s*).*$") {
        out=headers.replace_all(&out,"$1$2$3[redacted]").to_string();
    }
    if let Ok(kv)=Regex::new(r#"(?i)([\"']?(?:password|passwd|access[_-]?token|refresh[_-]?token|api[_-]?key|client[_-]?secret|secret)[\"']?\s*[:=]\s*)([\"'][^\"']*[\"']|[^,\s}\]]+)"#) {
        out=kv.replace_all(&out,"$1[redacted]").to_string();
    }
    out
}

fn snapshot_takeover_recommended(snapshot:&str)->bool{
    let h=snapshot.to_lowercase();
    ["captcha","verify you are human","are you human","security check","challenge required","two-factor authentication","2-step verification","enter verification code","confirm it's you","confirm it’s you"].iter().any(|x|h.contains(x))
}

fn filter_network_failures(raw:&str)->String{
    let status=Regex::new(r"(?i)(?:status(?:Code)?[=: ]+|\[)([45][0-9]{2})(?:\]|\b)").ok();
    let mut out=Vec::new();
    for line in raw.lines(){if status.as_ref().map(|r|r.is_match(line)).unwrap_or(false)||line.to_lowercase().contains("failed")||line.to_lowercase().contains("blocked"){out.push(line);}}
    if out.is_empty(){"No obvious 4xx/5xx/failed requests were present in the compact network listing.".into()}else{out.join("\n")}
}

async fn mcp_network_recent(headless:bool,limit:usize,failures_only:bool,preserve:bool)->Result<String>{
    let pages=if headless{headless_mcp_call("list_pages",vec![]).await?}else{chrome_mcp_call("list_pages",vec![]).await?};
    let page=chrome_mcp_page_id_from_list(&pages).ok_or_else(||anyhow!("No active browser page"))?;
    let args=vec![page.to_string(),"--pageSize".into(),limit.clamp(1,200).to_string(),"--pageIdx".into(),"0".into(),"--includePreservedRequests".into(),preserve.to_string()];
    let raw=if headless{headless_mcp_call("list_network_requests",args).await?}else{chrome_mcp_call("list_network_requests",args).await?};
    let result=if failures_only{filter_network_failures(&raw)}else{raw};
    Ok(serde_json::to_string_pretty(&json!({"headless":headless,"page_id":page,"failures_only":failures_only,"network":result}))?)
}

async fn mcp_network_request(headless:bool,request_id:u64)->Result<String>{
    let pages=if headless{headless_mcp_call("list_pages",vec![]).await?}else{chrome_mcp_call("list_pages",vec![]).await?};
    let page=chrome_mcp_page_id_from_list(&pages).ok_or_else(||anyhow!("No active browser page"))?;
    let args=vec![page.to_string(),"--reqid".into(),request_id.to_string()];
    let result=if headless{headless_mcp_call("get_network_request",args).await?}else{chrome_mcp_call("get_network_request",args).await?};
    let result=redact_sensitive_browser_text(&result);
    Ok(serde_json::to_string_pretty(&json!({"headless":headless,"page_id":page,"request_id":request_id,"request":result,"sensitive_headers_redacted":true}))?)
}

async fn mcp_console_messages(headless:bool,limit:usize,all_types:bool,preserve:bool)->Result<String>{
    let pages=if headless{headless_mcp_call("list_pages",vec![]).await?}else{chrome_mcp_call("list_pages",vec![]).await?};
    let page=chrome_mcp_page_id_from_list(&pages).ok_or_else(||anyhow!("No active browser page"))?;
    let mut args=vec![page.to_string(),"--pageSize".into(),limit.clamp(1,200).to_string(),"--pageIdx".into(),"0".into(),"--includePreservedMessages".into(),preserve.to_string()];
    if !all_types{args.extend(["--types".into(),"error".into(),"--types".into(),"warn".into()]);}
    let result=if headless{headless_mcp_call("list_console_messages",args).await?}else{chrome_mcp_call("list_console_messages",args).await?};
    let result=redact_sensitive_browser_text(&result);
    Ok(serde_json::to_string_pretty(&json!({"headless":headless,"page_id":page,"all_types":all_types,"console":result,"sensitive_values_redacted":true}))?)
}

async fn browser_diagnostics(limit:usize)->Result<String>{
    let network=mcp_network_recent(false,limit,true,true).await.unwrap_or_else(|e|format!("Network diagnostics unavailable: {e}"));
    let console=mcp_console_messages(false,limit,false,true).await.unwrap_or_else(|e|format!("Console diagnostics unavailable: {e}"));
    Ok(serde_json::to_string_pretty(&json!({"network_failures":network,"console_errors":console}))?)
}

async fn browser_memory_current(limit:usize)->Result<String>{
    let url=chrome_mcp_current_url().await?; Ok(serde_json::to_string_pretty(&browser_memory::current_summary(&url,limit.clamp(1,100)))?)
}

fn takeover_path()->PathBuf{dirs::data_local_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".local/share")).join("Fatir").join("browser-takeover.json")}
async fn browser_takeover(reason:&str)->Result<String>{
    let url=chrome_mcp_current_url().await.unwrap_or_default();let pages=chrome_mcp_call("list_pages",vec![]).await.unwrap_or_default();
    let checkpoint=json!({"active":true,"at":chrono::Utc::now().to_rfc3339(),"reason":reason,"url":url,"pages":pages});
    if let Some(parent)=takeover_path().parent(){fs::create_dir_all(parent)?;}fs::write(takeover_path(),serde_json::to_vec_pretty(&checkpoint)?)?;
    pointer::set_paused(true); let _=browser_cursor_hide().await;
    Ok(serde_json::to_string_pretty(&json!({"takeover":true,"reason":reason,"url":url,"message":"Fatir computer control is paused. Complete the browser step manually, then resume Fatir control."}))?)
}
fn browser_takeover_status()->Result<String>{
    let checkpoint=fs::read_to_string(takeover_path()).ok().and_then(|s|serde_json::from_str::<Value>(&s).ok()).unwrap_or_else(||json!({"active":false}));
    let status=pointer::status();Ok(serde_json::to_string_pretty(&json!({"paused":status.paused,"checkpoint":checkpoint}))?)
}
fn browser_takeover_resume()->Result<String>{
    pointer::set_paused(false);let checkpoint=fs::read_to_string(takeover_path()).ok().and_then(|s|serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null);
    if takeover_path().exists(){let _=fs::remove_file(takeover_path());}
    Ok(serde_json::to_string_pretty(&json!({"resumed":true,"previous_checkpoint":checkpoint,"instruction":"Re-snapshot the current page before acting; the user may have changed browser state during takeover."}))?)
}

async fn headless_browser_stop()->Result<String>{
    let result=chrome_mcp_exec_session(Some(HEADLESS_MCP_SESSION), &["stop".into()]).await;
    CHROME_MCP_HEADLESS_READY.store(false,Ordering::SeqCst);
    match result {
        Ok(v)=>Ok(serde_json::to_string_pretty(&json!({"stopped":true,"headless":true,"result":v}))?),
        Err(e)=>Ok(serde_json::to_string_pretty(&json!({"stopped":false,"headless":true,"message":format!("Headless session was not running or could not be stopped cleanly: {e}")}))?),
    }
}

async fn headless_browser_elements()->Result<String>{
    let (page,snapshot)=chrome_mcp_snapshot_for(true).await?;let url=chrome_mcp_url_for(true).await.unwrap_or_default();
    let human_intervention_required=snapshot_takeover_recommended(&snapshot);
    Ok(serde_json::to_string_pretty(&json!({"engine":"chrome-devtools-mcp","headless":true,"page_id":page,"url":url,"snapshot":snapshot,"human_intervention_required":human_intervention_required,"instruction":if human_intervention_required{"Do not switch to visible browsing automatically. Report that this challenge requires an explicitly requested visible/manual run."}else{""}}))?)
}
async fn headless_browser_open_url(url:&str)->Result<String>{
    let parsed=url::Url::parse(url).map_err(|_|anyhow!("Invalid URL: {url}"))?;if !matches!(parsed.scheme(),"http"|"https"){return Err(anyhow!("Only http/https URLs are supported"));}
    let pages=headless_mcp_call("list_pages",vec![]).await?;let page=chrome_mcp_page_id_from_list(&pages).ok_or_else(||anyhow!("Headless browser has no page"))?;
    let out=headless_mcp_call("navigate_page",vec![page.to_string(),"--url".into(),url.to_string(),"--timeout".into(),"12000".into()]).await?;
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"headless":true,"page_id":page,"url":url,"result":out}))?)
}
async fn headless_browser_click_text(text:&str,exact:bool)->Result<String>{
    let url=chrome_mcp_url_for(true).await.unwrap_or_default();
    let (page,snapshot)=chrome_mcp_snapshot_for(true).await?;let _=browser_memory::remember_snapshot(&url,page,&snapshot);
    let mut matched=chrome_mcp_uid(&snapshot,text,exact,&["button","link","menuitem","tab"]);let mut healed=false;
    if matched.is_none() && !exact {for remembered in browser_memory::candidates(&url,"click",text,8){if let Some(v)=chrome_mcp_uid(&snapshot,&remembered.label,false,&[remembered.role.as_str(),"button","link","menuitem","tab"]){matched=Some(v);healed=true;break;}}}
    let (uid,role,label)=matched.ok_or_else(||anyhow!("Headless browser could not find a control matching '{text}'"))?;
    let out=headless_mcp_call("click",vec![page.to_string(),uid.clone(),"--includeSnapshot".into(),"true".into()]).await?;
    let _=browser_memory::record_target(&url,"click",&role,&label,true,Some(text));
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"headless":true,"page_id":page,"uid":uid,"role":role,"clicked":label,"self_healed":healed,"result":out}))?)
}
async fn headless_browser_fill_by_label(label:&str,text:&str)->Result<String>{
    let url=chrome_mcp_url_for(true).await.unwrap_or_default();
    let (page,snapshot)=chrome_mcp_snapshot_for(true).await?;let _=browser_memory::remember_snapshot(&url,page,&snapshot);
    let mut matched=chrome_mcp_uid(&snapshot,label,false,&["textbox","searchbox","combobox","spinbutton"]);let mut healed=false;
    if matched.is_none(){for remembered in browser_memory::candidates(&url,"fill",label,8){if let Some(v)=chrome_mcp_uid(&snapshot,&remembered.label,false,&[remembered.role.as_str(),"textbox","searchbox","combobox","spinbutton"]){matched=Some(v);healed=true;break;}}}
    let (uid,role,found)=matched.ok_or_else(||anyhow!("Headless browser could not find a field matching '{label}'"))?;
    let out=headless_mcp_call("fill",vec![page.to_string(),uid.clone(),text.to_string(),"--includeSnapshot".into(),"true".into()]).await?;
    let _=browser_memory::record_target(&url,"fill",&role,&found,true,Some(label));
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"headless":true,"page_id":page,"uid":uid,"role":role,"field":found,"characters":text.chars().count(),"self_healed":healed,"result":out}))?)
}
async fn headless_browser_observe()->Result<String>{
    let pages=headless_mcp_call("list_pages",vec![]).await?;let page=chrome_mcp_page_id_from_list(&pages).ok_or_else(||anyhow!("Headless browser has no page"))?;
    let dir=dirs::picture_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join("Pictures")).join("Fatir").join("Headless");fs::create_dir_all(&dir)?;
    let file=dir.join(format!("headless-{}.png",chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")));
    headless_mcp_call("take_screenshot",vec![page.to_string(),"--filePath".into(),file.display().to_string()]).await?;
    if !file.is_file(){return Err(anyhow!("Headless MCP did not produce the screenshot"));}
    let url=chrome_mcp_url_for(true).await.unwrap_or_default();Ok(serde_json::to_string(&json!({"screenshot":file.display().to_string(),"url":url,"page_id":page,"headless":true,"automation":"Chrome DevTools MCP isolated headless session"}))?)
}

async fn browser_targets() -> Result<Vec<Value>> {
    ensure_fatir_browser().await?;
    let value: Value = Client::new()
        .get(format!("http://127.0.0.1:{CDP_PORT}/json/list"))
        .send().await?.error_for_status()?.json().await?;
    Ok(value.as_array().cloned().unwrap_or_default())
}

async fn browser_target() -> Result<Value> {
    let targets = browser_targets().await?;
    let preferred_url = if CHROME_MCP_READY.load(Ordering::SeqCst) { chrome_mcp_current_url().await.ok() } else { None };
    if let Some(ref wanted)=preferred_url {
        if let Some(target)=targets.iter().find(|t| {
            t.get("type").and_then(Value::as_str)==Some("page")
                && t.get("webSocketDebuggerUrl").and_then(Value::as_str).is_some()
                && t.get("url").and_then(Value::as_str)==Some(wanted.as_str())
        }) { return Ok(target.clone()); }
    }
    targets.into_iter()
        .find(|t| t.get("type").and_then(Value::as_str) == Some("page") && t.get("webSocketDebuggerUrl").and_then(Value::as_str).is_some())
        .ok_or_else(|| anyhow!("Fatir controlled browser has no active webpage tab"))
}

type CdpSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn cdp_connect() -> Result<CdpSocket> {
    let target = browser_target().await?;
    let ws_url = target.get("webSocketDebuggerUrl").and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Browser target has no DevTools websocket"))?;
    let (socket, _) = connect_async(ws_url).await.context("Could not connect to Fatir browser DevTools")?;
    Ok(socket)
}

async fn cdp_send_collect(socket: &mut CdpSocket, id: u64, method: &str, params: Value, events: &mut Vec<Value>) -> Result<Value> {
    let payload = json!({"id":id,"method":method,"params":params});
    socket.send(Message::Text(payload.to_string())).await?;
    while let Some(msg) = socket.next().await {
        let msg = msg?;
        if !msg.is_text() { continue; }
        let value: Value = serde_json::from_str(msg.to_text()?)?;
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            if let Some(err) = value.get("error") { return Err(anyhow!("Browser automation error: {err}")); }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
        if value.get("method").is_some() { events.push(value); }
    }
    Err(anyhow!("Browser automation connection closed unexpectedly"))
}

async fn cdp_drain_events(socket: &mut CdpSocket, events: &mut Vec<Value>, rounds: usize) -> Result<()> {
    for _ in 0..rounds {
        match tokio::time::timeout(std::time::Duration::from_millis(90), socket.next()).await {
            Ok(Some(Ok(msg))) if msg.is_text() => {
                let value: Value = serde_json::from_str(msg.to_text()?)?;
                if value.get("method").is_some() { events.push(value); }
            }
            Ok(Some(Err(err))) => return Err(err.into()),
            _ => break,
        }
    }
    Ok(())
}

fn file_chooser_backend_node(events: &[Value]) -> Option<i64> {
    events.iter().rev().find_map(|event| {
        if event.get("method").and_then(Value::as_str) == Some("Page.fileChooserOpened") {
            event.pointer("/params/backendNodeId").and_then(Value::as_i64)
        } else { None }
    })
}

async fn cdp_command(method: &str, params: Value) -> Result<Value> {
    if active_browser_session_requested() {
        return Err(anyhow!("Direct CDP fallback is disabled for active-browser-session mode because it would target Fatir's managed Chrome instead of the user's current Chrome. Use Chrome DevTools MCP semantic tools for this turn."));
    }
    let mut socket = cdp_connect().await?;
    let mut events = Vec::new();
    cdp_send_collect(&mut socket, 71, method, params, &mut events).await
}

async fn browser_viewport() -> Result<(i64, i64, String, String)> {
    let expression = "({width:window.innerWidth,height:window.innerHeight,title:document.title,url:location.href})";
    let out = cdp_command("Runtime.evaluate", json!({"expression":expression,"returnByValue":true})).await?;
    let value = out.pointer("/result/value").cloned().unwrap_or_else(|| json!({}));
    Ok((
        value.get("width").and_then(Value::as_i64).unwrap_or(1280),
        value.get("height").and_then(Value::as_i64).unwrap_or(800),
        value.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
        value.get("url").and_then(Value::as_str).unwrap_or("").to_string(),
    ))
}

async fn browser_cursor_hide() -> Result<()> {
    let script = r#"(() => {
      const c=document.getElementById('fatir-virtual-cursor');
      const r=document.getElementById('fatir-target-ring');
      if(c){ if(c.__fatirHideTimer)clearTimeout(c.__fatirHideTimer); c.style.display='none'; c.style.opacity='0'; }
      if(r)r.style.display='none';
      return true;
    })()"#;
    // There may be no controlled browser yet; callers intentionally treat that as best-effort.
    cdp_command("Runtime.evaluate", json!({"expression":script,"returnByValue":true})).await.map(|_| ())
}

pub async fn hide_all_virtual_pointers() {
    let _ = browser_cursor_hide().await;
    let _ = pointer::hide();
}

async fn browser_cursor_move(x: i64, y: i64, label: &str, width: i64, height: i64) -> Result<()> {
    pointer::enter_browser()?;
    let label_js=serde_json::to_string(label)?;
    let script=format!(r#"(() => {{
      const x={x},y={y},w={width},h={height},label={label_js};
      let style=document.getElementById('fatir-cursor-style');
      if(!style){{style=document.createElement('style');style.id='fatir-cursor-style';style.textContent=`
        #fatir-virtual-cursor{{position:fixed;left:0;top:0;z-index:2147483647;pointer-events:none;transform:translate3d(64px,64px,0);transition:transform .42s cubic-bezier(.22,.84,.26,1),opacity .16s ease;filter:drop-shadow(0 2px 4px rgba(0,0,0,.25));font-family:system-ui,-apple-system,sans-serif}}
        #fatir-virtual-cursor .fatir-arrow{{position:absolute;left:0;top:0;width:0;height:0;border-left:8px solid transparent;border-right:8px solid transparent;border-bottom:24px solid #16866e;transform:rotate(-40deg);transform-origin:50% 75%;filter:drop-shadow(0 0 1px #fff) drop-shadow(0 0 1px #fff)}}
        #fatir-virtual-cursor .fatir-label{{position:absolute;left:18px;top:12px;white-space:nowrap;background:rgba(18,21,25,.92);color:white;border:1px solid rgba(255,255,255,.16);border-radius:9px;padding:5px 8px;font-size:11px;font-weight:650;letter-spacing:.01em;box-shadow:0 4px 14px rgba(0,0,0,.18)}}
        #fatir-target-ring{{position:fixed;z-index:2147483646;pointer-events:none;border:2px solid rgba(22,134,110,.78);border-radius:9px;box-shadow:0 0 0 3px rgba(22,134,110,.12);transition:all .22s ease;}}
      `;document.documentElement.appendChild(style);}}
      let c=document.getElementById('fatir-virtual-cursor');
      if(!c){{c=document.createElement('div');c.id='fatir-virtual-cursor';c.innerHTML='<span class="fatir-arrow"></span><span class="fatir-label"></span>';document.documentElement.appendChild(c);}}
      c.querySelector('.fatir-label').textContent=label||'Fatir';
      if(c.__fatirHideTimer)clearTimeout(c.__fatirHideTimer);
      c.style.display='block'; c.style.opacity='1';
      requestAnimationFrame(()=>{{c.style.transform=`translate3d(${{Math.round(x-7)}}px,${{Math.round(y-5)}}px,0)`;}});
      let ring=document.getElementById('fatir-target-ring');
      if(w>1&&h>1){{if(!ring){{ring=document.createElement('div');ring.id='fatir-target-ring';document.documentElement.appendChild(ring);}}ring.style.display='block';ring.style.left=Math.round(x-w/2-3)+'px';ring.style.top=Math.round(y-h/2-3)+'px';ring.style.width=Math.round(w+6)+'px';ring.style.height=Math.round(h+6)+'px';}}
      else if(ring) ring.style.display='none';
      c.__fatirHideTimer=setTimeout(()=>{{c.style.opacity='0'; if(ring)ring.style.display='none';}},1800);
      return true;
    }})()"#);
    let _=cdp_command("Runtime.evaluate",json!({"expression":script,"returnByValue":true})).await?;
    tokio::time::sleep(std::time::Duration::from_millis(360)).await;
    cdp_command("Input.dispatchMouseEvent",json!({"type":"mouseMoved","x":x,"y":y,"button":"none"})).await?;
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    Ok(())
}

async fn browser_virtual_click(x: i64, y: i64, label: &str, width: i64, height: i64) -> Result<()> {
    browser_cursor_move(x,y,label,width,height).await?;
    cdp_command("Input.dispatchMouseEvent", json!({"type":"mousePressed","x":x,"y":y,"button":"left","clickCount":1})).await?;
    tokio::time::sleep(std::time::Duration::from_millis(75)).await;
    cdp_command("Input.dispatchMouseEvent", json!({"type":"mouseReleased","x":x,"y":y,"button":"left","clickCount":1})).await?;
    tokio::time::sleep(std::time::Duration::from_millis(360)).await;
    Ok(())
}

async fn browser_cursor_status(label: &str) -> Result<()> {
    let (width,height,_,_)=browser_viewport().await?;
    browser_cursor_move((width-100).max(40),(height/10).clamp(30,80),label,0,0).await
}

async fn browser_observe() -> Result<String> {
    if let Ok(result)=chrome_mcp_screenshot().await { return Ok(result); }
    ensure_fatir_browser().await?;
    let _ = cdp_command("Page.enable", json!({})).await;
    let shot = cdp_command("Page.captureScreenshot", json!({"format":"png","fromSurface":true,"captureBeyondViewport":false})).await?;
    let encoded = shot.get("data").and_then(Value::as_str).ok_or_else(|| anyhow!("Browser screenshot returned no image"))?;
    let bytes = STANDARD.decode(encoded)?;
    let dir = dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures")).join("Fatir").join("Browser");
    fs::create_dir_all(&dir)?;
    let file = dir.join(format!("browser-{}.png", chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")));
    fs::write(&file, bytes)?;
    let (width,height,title,url) = browser_viewport().await?;
    Ok(serde_json::to_string(&json!({
        "screenshot": file.display().to_string(),
        "title": title,
        "url": url,
        "width": width,
        "height": height,
        "automation": "Chrome DevTools Protocol + Fatir virtual pointer",
        "virtual_pointer": true,
        "physical_mouse_moved": false,
        "coordinate_system": "Coordinates are webpage viewport coordinates. Fatir moves its own visible pointer to browser targets; the physical mouse remains free. Prefer browser_elements and browser_click_element; use browser_click only as a fallback."
    }))?)
}

async fn browser_elements() -> Result<String> {
    if let Ok(result)=chrome_mcp_elements().await { return Ok(result); }
    ensure_fatir_browser().await?;
    let script = r#"(() => {
      let n=0;
      window.__fatirLocators = {};
      const visible = el => {
        const r=el.getBoundingClientRect();
        const s=getComputedStyle(el);
        return r.width>1 && r.height>1 && s.visibility!=='hidden' && s.display!=='none' && r.bottom>=0 && r.right>=0 && r.top<=innerHeight && r.left<=innerWidth;
      };
      const clean = v => (v||'').replace(/\s+/g,' ').trim();
      const label = el => clean(el.innerText || el.getAttribute('aria-label') || el.getAttribute('title') || el.getAttribute('placeholder') || el.value || '').slice(0,240);
      return [...document.querySelectorAll('a,button,input,textarea,select,[role="button"],[role="link"],[role="checkbox"],[role="radio"],[role="tab"],[role="menuitem"],[contenteditable="true"],[onclick]')]
        .filter(visible).slice(0,90).map(el => {
          const id='fatir-'+(++n); el.setAttribute('data-fatir-id',id); const r=el.getBoundingClientRect();
          const loc={tag:el.tagName.toLowerCase(),type:el.getAttribute('type')||'',role:el.getAttribute('role')||'',text:label(el),aria:clean(el.getAttribute('aria-label')),title:clean(el.getAttribute('title')),placeholder:clean(el.getAttribute('placeholder')),name:clean(el.getAttribute('name')),href:el.href||''};
          window.__fatirLocators[id]=loc;
          return {id,...loc,disabled:!!el.disabled,checked:!!el.checked,x:Math.round(r.x),y:Math.round(r.y),width:Math.round(r.width),height:Math.round(r.height)};
        });
    })()"#;
    let out = cdp_command("Runtime.evaluate", json!({"expression":script,"returnByValue":true,"awaitPromise":true})).await?;
    let value = out.pointer("/result/value").cloned().unwrap_or_else(|| json!([]));
    Ok(serde_json::to_string_pretty(&value)?)
}

async fn browser_click_text(text: &str, exact: bool) -> Result<String> {
    if text.trim().is_empty() { return Err(anyhow!("Control text cannot be empty")); }
    if let Ok(result)=chrome_mcp_click_text(text,exact).await { return Ok(result); }
    pointer::ensure_enabled()?;
    ensure_fatir_browser().await?;
    let wanted = serde_json::to_string(text.trim())?;
    let exact_js = if exact { "true" } else { "false" };
    let script = format!(r#"(() => {{
      const wanted={wanted}.toLowerCase().replace(/\s+/g,' ').trim();
      const exact={exact_js};
      const visible=el=>{{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>1&&r.height>1&&s.visibility!=='hidden'&&s.display!=='none'&&Number(s.opacity||1)>0.02;}};
      const clean=s=>(s||'').replace(/\s+/g,' ').trim();
      const clickable=el=>el.closest('button,a,[role="button"],[role="link"],input[type="button"],input[type="submit"],[onclick],[tabindex]')||el;
      const namesOf=el=>[
        el.getAttribute('aria-label'), el.getAttribute('title'), el.getAttribute('data-testid'),
        el.getAttribute('data-icon'), el.getAttribute('name'), el.value, el.innerText,
        el.querySelector?.('[aria-label]')?.getAttribute('aria-label'),
        el.querySelector?.('[data-icon]')?.getAttribute('data-icon')
      ].filter(Boolean).map(clean).filter(Boolean);
      const raw=[...document.querySelectorAll('button,a,[role="button"],[role="link"],input[type="button"],input[type="submit"],[onclick],[tabindex],[aria-label],[title],[data-testid],[data-icon]')].filter(visible);
      const seen=new Set(); const ranked=[];
      raw.forEach((source,i)=>{{
        const el=clickable(source); if(!visible(el))return;
        const key=el; if(seen.has(key))return; seen.add(key);
        const names=[...namesOf(source),...(source===el?[]:namesOf(el))];
        const lowers=names.map(x=>x.toLowerCase()); const t=names.join(' ');
        let score=0;
        if(lowers.some(x=>x===wanted))score=120;
        else if(!exact&&lowers.some(x=>x.includes(wanted)))score=80;
        else if(!exact&&lowers.some(x=>wanted.includes(x)&&x.length>2))score=48;
        if(score<=0)return;
        if(el.disabled||el.getAttribute('aria-disabled')==='true')score-=100;
        const r=el.getBoundingClientRect();
        if(r.left>window.innerWidth*.55)score+=6;
        if(r.top>window.innerHeight*.55)score+=6;
        ranked.push({{el,t:clean(t),score,i,r}});
      }});
      ranked.sort((a,b)=>b.score-a.score||a.i-b.i);
      if(!ranked.length)return{{ok:false,error:'No visible control matched',query:wanted}};
      const hit=ranked[0]; hit.el.scrollIntoView({{block:'center',inline:'center'}}); const r=hit.el.getBoundingClientRect();
      return{{ok:true,text:hit.t||wanted,tag:hit.el.tagName.toLowerCase(),x:Math.round(r.left+r.width/2),y:Math.round(r.top+r.height/2),width:Math.round(r.width),height:Math.round(r.height),score:hit.score}};
    }})()"#);
    let out=cdp_command("Runtime.evaluate",json!({"expression":script,"returnByValue":true})).await?;
    let value=out.pointer("/result/value").cloned().unwrap_or_else(||json!({}));
    if value.get("ok").and_then(Value::as_bool)!=Some(true){return Err(anyhow!("{}",value.get("error").and_then(Value::as_str).unwrap_or("Could not find requested webpage control")));}
    let x=value.get("x").and_then(Value::as_i64).ok_or_else(||anyhow!("Matched browser control has no x coordinate"))?;
    let y=value.get("y").and_then(Value::as_i64).ok_or_else(||anyhow!("Matched browser control has no y coordinate"))?;
    let w=value.get("width").and_then(Value::as_i64).unwrap_or(1);
    let h=value.get("height").and_then(Value::as_i64).unwrap_or(1);
    let label=value.get("text").and_then(Value::as_str).filter(|v|!v.is_empty()).unwrap_or(text);
    browser_virtual_click(x,y,&format!("Fatir · {}",label.chars().take(26).collect::<String>()),w,h).await?;
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"clicked":label,"x":x,"y":y,"virtual_pointer":true,"physical_mouse_moved":false}))?)
}

async fn browser_fill_by_label(label: &str, text: &str) -> Result<String> {
    if label.trim().is_empty(){return Err(anyhow!("Field label cannot be empty"));}
    if text.chars().count()>20_000{return Err(anyhow!("Text is too long for browser input"));}
    if let Ok(result)=chrome_mcp_fill_by_label(label,text).await { return Ok(result); }
    pointer::ensure_enabled()?;
    ensure_fatir_browser().await?;
    let wanted=serde_json::to_string(label.trim())?;
    let script=format!(r#"(() => {{
      const wanted={wanted}.toLowerCase().replace(/\s+/g,' ').trim();
      const visible=el=>{{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>1&&r.height>1&&s.visibility!=='hidden'&&s.display!=='none';}};
      const fields=[...document.querySelectorAll('input:not([type="hidden"]),textarea,[contenteditable="true"]')].filter(visible);
      const labelFor=el=>{{let a=[el.getAttribute('aria-label'),el.getAttribute('placeholder'),el.getAttribute('name'),el.getAttribute('id')].filter(Boolean).join(' ');if(el.id){{const l=document.querySelector('label[for="'+CSS.escape(el.id)+'"]');if(l)a+=' '+(l.innerText||'');}}const parent=el.closest('label');if(parent)a+=' '+(parent.innerText||'');return a.replace(/\s+/g,' ').trim();}};
      const ranked=fields.map((el,i)=>{{const t=labelFor(el),l=t.toLowerCase();let score=l===wanted?100:l.includes(wanted)?70:wanted.includes(l)&&l.length>2?40:0;return{{el,t,score,i}};}}).filter(x=>x.score>0).sort((a,b)=>b.score-a.score||a.i-b.i);
      if(!ranked.length)return{{ok:false,error:'No visible field matched',query:wanted}};
      document.querySelectorAll('[data-fatir-fill-target]').forEach(x=>x.removeAttribute('data-fatir-fill-target'));
      const el=ranked[0].el; el.scrollIntoView({{block:'center',inline:'center'}}); el.setAttribute('data-fatir-fill-target','1'); const r=el.getBoundingClientRect();
      return{{ok:true,label:ranked[0].t,x:Math.round(r.left+r.width/2),y:Math.round(r.top+r.height/2),width:Math.round(r.width),height:Math.round(r.height)}};
    }})()"#);
    let out=cdp_command("Runtime.evaluate",json!({"expression":script,"returnByValue":true})).await?;
    let v=out.pointer("/result/value").cloned().unwrap_or_else(||json!({}));
    if v.get("ok").and_then(Value::as_bool)!=Some(true){return Err(anyhow!("{}",v.get("error").and_then(Value::as_str).unwrap_or("Could not find requested field")));}
    let x=v.get("x").and_then(Value::as_i64).ok_or_else(||anyhow!("Matched browser field has no x coordinate"))?;
    let y=v.get("y").and_then(Value::as_i64).ok_or_else(||anyhow!("Matched browser field has no y coordinate"))?;
    let w=v.get("width").and_then(Value::as_i64).unwrap_or(1); let h=v.get("height").and_then(Value::as_i64).unwrap_or(1);
    browser_virtual_click(x,y,&format!("Fatir · {}",label.chars().take(24).collect::<String>()),w,h).await?;
    browser_key("ctrl+a").await?; browser_key("Backspace").await?; browser_type(text).await?;
    let _=cdp_command("Runtime.evaluate",json!({"expression":"document.querySelector('[data-fatir-fill-target]')?.removeAttribute('data-fatir-fill-target')","returnByValue":true})).await;
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"field":label,"characters":text.chars().count(),"virtual_pointer":true,"physical_mouse_moved":false}))?)
}

async fn browser_page_summary(max_chars: usize) -> Result<String> {
    let max_chars=max_chars.clamp(1000,20_000);
    if let Ok((page,snapshot))=chrome_mcp_snapshot().await {
        let takeover_recommended=snapshot_takeover_recommended(&snapshot);
        let summary: String=snapshot.chars().take(max_chars).collect();
        return Ok(serde_json::to_string_pretty(&json!({"engine":"chrome-devtools-mcp","page_id":page,"snapshot":summary,"human_takeover_recommended":takeover_recommended}))?);
    }
    ensure_fatir_browser().await?;
    let script=format!(r#"(() => {{
      const clean=s=>(s||'').replace(/\s+/g,' ').trim();
      const visible=el=>{{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>1&&r.height>1&&s.visibility!=='hidden'&&s.display!=='none';}};
      const headings=[...document.querySelectorAll('h1,h2,h3')].filter(visible).slice(0,30).map(el=>clean(el.innerText)).filter(Boolean);
      const controls=[...document.querySelectorAll('button,a,input,textarea,select,[role="button"],[role="link"]')].filter(visible).slice(0,70).map(el=>({{tag:el.tagName.toLowerCase(),text:clean(el.innerText||el.getAttribute('aria-label')||el.getAttribute('placeholder')||el.value).slice(0,180),href:el.href||'',type:el.getAttribute('type')||''}})).filter(x=>x.text||x.href);
      const text=clean(document.body?document.body.innerText:'').slice(0,{max_chars});
      return{{title:document.title,url:location.href,headings,controls,text}};
    }})()"#);
    let out=cdp_command("Runtime.evaluate",json!({"expression":script,"returnByValue":true})).await?;
    Ok(serde_json::to_string_pretty(&out.pointer("/result/value").cloned().unwrap_or_else(||json!({})))?)
}

async fn wait_browser_ready(timeout_ms: u64) -> Result<()> {
    let started=std::time::Instant::now();
    loop{
        let out=cdp_command("Runtime.evaluate",json!({"expression":"document.readyState","returnByValue":true})).await;
        if let Ok(v)=out{if matches!(v.pointer("/result/value").and_then(Value::as_str),Some("complete")|Some("interactive")){tokio::time::sleep(std::time::Duration::from_millis(350)).await;return Ok(());}}
        if started.elapsed()>std::time::Duration::from_millis(timeout_ms){return Ok(());}
        tokio::time::sleep(std::time::Duration::from_millis(180)).await;
    }
}

fn csv_escape(value: &str) -> String { format!("\"{}\"", value.replace('"', "\"\"")) }

async fn amazon_keyword_research(keyword: &str, limit: usize, output_path: Option<&str>) -> Result<String> {
    let keyword=keyword.trim(); if keyword.is_empty(){return Err(anyhow!("Keyword cannot be empty"));}
    let limit=limit.clamp(1,10);
    let mut u=url::Url::parse("https://www.amazon.com/s")?; u.query_pairs_mut().append_pair("k",keyword);
    browser_open_url(u.as_str()).await?; wait_browser_ready(7000).await?;
    let _=browser_cursor_status("Fatir · Reading results").await;
    let search_script=r#"(() => [...document.querySelectorAll('div[data-component-type="s-search-result"][data-asin]')].map((card,idx)=>{const asin=(card.getAttribute('data-asin')||'').trim();const title=(card.querySelector('h2 span')?.innerText||card.querySelector('h2')?.innerText||'').replace(/\s+/g,' ').trim();const a=card.querySelector('h2 a')||card.querySelector('a.a-link-normal.s-no-outline');const href=a?.href||'';const price=(card.querySelector('.a-price .a-offscreen')?.innerText||'').trim();const sponsored=/sponsored/i.test(card.innerText||'');return{position:idx+1,asin,title,href,search_price:price,sponsored};}).filter(x=>x.asin&&x.title&&x.href&&!x.sponsored))()"#;
    let out=cdp_command("Runtime.evaluate",json!({"expression":search_script,"returnByValue":true})).await?;
    let cards=out.pointer("/result/value").and_then(Value::as_array).cloned().unwrap_or_default();
    if cards.is_empty(){return Err(anyhow!("Amazon returned no readable organic product cards. The page may require verification or its layout may have changed."));}
    let selected:Vec<Value>=cards.into_iter().take(limit).collect();
    let mut rows=Vec::new();
    for (i,card) in selected.iter().enumerate(){
        let href=card.get("href").and_then(Value::as_str).unwrap_or(""); if href.is_empty(){continue;}
        cdp_command("Page.navigate",json!({"url":href})).await?; wait_browser_ready(6500).await?;
        let _=browser_cursor_status(&format!("Fatir · Product {}/{}",i+1,limit)).await;
        let detail_script=r#"(() => {const clean=s=>(s||'').replace(/\s+/g,' ').trim();const bullets=[...document.querySelectorAll('#feature-bullets li span.a-list-item')].map(x=>clean(x.innerText)).filter(x=>x&&!/make sure this fits/i.test(x)).slice(0,12);const price=(document.querySelector('#corePrice_feature_div .a-offscreen')?.innerText||document.querySelector('#corePriceDisplay_desktop_feature_div .a-offscreen')?.innerText||document.querySelector('.a-price .a-offscreen')?.innerText||'').trim();const title=clean(document.querySelector('#productTitle')?.innerText)||document.title;const asin=(document.querySelector('#ASIN')?.value||document.querySelector('input[name="ASIN"]')?.value||'').trim();const brand=clean(document.querySelector('#bylineInfo')?.innerText);const seller=clean(document.querySelector('#sellerProfileTriggerId')?.innerText||document.querySelector('#merchant-info')?.innerText);return{title,asin,price,brand,seller,bullets,url:location.href,blocked:/robot check|enter the characters you see|captcha/i.test(document.body?.innerText||'')};})()"#;
        let d=cdp_command("Runtime.evaluate",json!({"expression":detail_script,"returnByValue":true})).await?;
        let mut detail=d.pointer("/result/value").cloned().unwrap_or_else(||json!({}));
        if detail.get("asin").and_then(Value::as_str).unwrap_or("").is_empty(){detail["asin"]=card.get("asin").cloned().unwrap_or(Value::String(String::new()));}
        if detail.get("price").and_then(Value::as_str).unwrap_or("").is_empty(){detail["price"]=card.get("search_price").cloned().unwrap_or(Value::String(String::new()));}
        detail["position"]=json!(i+1); rows.push(detail);
    }
    if rows.is_empty(){return Err(anyhow!("Could not read any Amazon product detail pages."));}
    let path=if let Some(p)=output_path{expand_path(p)}else{let dir=dirs::download_dir().unwrap_or_else(||dirs::home_dir().unwrap_or_default().join("Downloads")).join("Fatir");fs::create_dir_all(&dir)?;let slug=keyword.chars().map(|c|if c.is_ascii_alphanumeric(){c.to_ascii_lowercase()}else{'-'}).collect::<String>().split('-').filter(|x|!x.is_empty()).take(8).collect::<Vec<_>>().join("-");dir.join(format!("amazon-{}-{}.csv",if slug.is_empty(){"research"}else{&slug},chrono::Local::now().format("%Y%m%d-%H%M%S"))) };
    if let Some(parent)=path.parent(){fs::create_dir_all(parent)?;}
    if path.exists(){return Err(anyhow!("Output file already exists; choose a new CSV path so Fatir does not overwrite it."));}
    let mut csv=String::from("position,asin,price,title,brand,seller,bullets,url\n");
    for row in &rows{let bullets=row.get("bullets").and_then(Value::as_array).map(|a|a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" | ")).unwrap_or_default();let vals=[row.get("position").and_then(Value::as_u64).unwrap_or(0).to_string(),row.get("asin").and_then(Value::as_str).unwrap_or("").to_string(),row.get("price").and_then(Value::as_str).unwrap_or("").to_string(),row.get("title").and_then(Value::as_str).unwrap_or("").to_string(),row.get("brand").and_then(Value::as_str).unwrap_or("").to_string(),row.get("seller").and_then(Value::as_str).unwrap_or("").to_string(),bullets,row.get("url").and_then(Value::as_str).unwrap_or("").to_string()];csv.push_str(&vals.iter().map(|v|csv_escape(v)).collect::<Vec<_>>().join(","));csv.push('\n');}
    fs::write(&path,csv)?;
    Ok(serde_json::to_string_pretty(&json!({"keyword":keyword,"count":rows.len(),"csv_path":path.display().to_string(),"products":rows,"note":"Collected through one deterministic Fatir browser skill to minimize model/tool round-trips."}))?)
}

async fn browser_upload_file(path: &str, hint: Option<&str>) -> Result<String> {
    pointer::enter_browser()?;
    ensure_fatir_browser().await?;
    let expanded=expand_path(path);
    if !expanded.is_file() { return Err(anyhow!("Upload file does not exist or is not a regular file: {}", expanded.display())); }
    let canonical=fs::canonicalize(&expanded).with_context(||format!("Could not resolve upload file {}",expanded.display()))?;
    let filename=canonical.file_name().and_then(|x|x.to_str()).unwrap_or("file").to_string();
    let ext=canonical.extension().and_then(|x|x.to_str()).unwrap_or("").to_ascii_lowercase();
    let kind = if ["png","jpg","jpeg","gif","webp","bmp","svg"].contains(&ext.as_str()) { "image" }
        else if ["mp4","mov","mkv","webm","avi"].contains(&ext.as_str()) { "video" }
        else { "document" };
    let hint_js=serde_json::to_string(hint.unwrap_or(""))?;
    let kind_js=serde_json::to_string(kind)?;
    let path_text=canonical.display().to_string();

    // Keep one DevTools session for object-backed file inputs. Remote object IDs are session-scoped;
    // carrying one across separate websocket connections is what caused the old "Could not find object" failures.
    let mut socket=cdp_connect().await?;
    let mut events=Vec::new();
    let mut seq=9000u64;
    seq+=1; let _=cdp_send_collect(&mut socket,seq,"Page.enable",json!({}),&mut events).await;
    seq+=1; let intercept=cdp_send_collect(&mut socket,seq,"Page.setInterceptFileChooserDialog",json!({"enabled":true}),&mut events).await.is_ok();

    let input_expression=format!(r#"(() => {{
      const hint={hint_js}.toLowerCase().trim(), kind={kind_js};
      const inputs=[];
      const scan=root=>{{
        try{{root.querySelectorAll('input[type=file]').forEach(x=>inputs.push(x));}}catch{{}}
        try{{root.querySelectorAll('*').forEach(el=>{{if(el.shadowRoot)scan(el.shadowRoot);if(el.tagName==='IFRAME'){{try{{if(el.contentDocument)scan(el.contentDocument);}}catch{{}}}}}});}}catch{{}}
      }};
      scan(document);
      if(!inputs.length)return null;
      const clean=v=>(v||'').replace(/\s+/g,' ').trim().toLowerCase();
      const score=el=>{{
        const accept=clean(el.getAttribute('accept'));
        const id=el.id||'';
        let lab=null; try{{lab=id?document.querySelector(`label[for="${{CSS.escape(id)}}"]`):null;}}catch{{}}
        const nearby=clean((lab&&lab.innerText)||el.getAttribute('aria-label')||el.getAttribute('title')||el.getAttribute('name')||el.getAttribute('data-testid')||accept);
        let s=1;
        if(hint && nearby.includes(hint))s+=120;
        if(kind==='image' && (accept.includes('image')||accept.includes('.jpg')||accept.includes('.png')||nearby.includes('photo')||nearby.includes('image')))s+=70;
        if(kind==='video' && (accept.includes('video')||nearby.includes('video')))s+=70;
        if(kind==='document'){{
          const wildcard=!accept||accept==='*'||accept==='*/*';
          const documentish=accept.includes('pdf')||accept.includes('document')||accept.includes('.doc')||accept.includes('.zip')||accept.includes('.txt')||accept.includes('.csv')||accept.includes('.xls')||accept.includes('.ppt')||nearby.includes('document')||nearby.includes('file');
          const mediaOnly=!wildcard&&!documentish&&(accept.includes('image')||accept.includes('video')||accept.includes('audio'));
          if(wildcard)s+=110;
          if(documentish)s+=85;
          if(mediaOnly)s-=140;
        }}
        if(el.multiple)s+=3;
        return s;
      }};
      inputs.sort((a,b)=>score(b)-score(a));
      return inputs[0];
    }})()"#);

    let launcher_expression=format!(r#"(() => {{
      const hint={hint_js}.toLowerCase().trim(), kind={kind_js};
      const clean=v=>(v||'').replace(/\s+/g,' ').trim();
      const visible=el=>{{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>1&&r.height>1&&s.visibility!=='hidden'&&s.display!=='none'&&r.bottom>=0&&r.right>=0&&r.top<=innerHeight&&r.left<=innerWidth;}};
      const els=[...document.querySelectorAll('button,a,label,[role="button"],[role="menuitem"],[onclick]')].filter(visible).filter(el=>el.getAttribute('data-fatir-attach-tried')!=='1');
      const words=['attach','attachment','upload','add file','choose file','paperclip','document','file','photo','image','media','video'];
      const ranked=els.map((el,i)=>{{
        const t=clean([el.innerText,el.getAttribute('aria-label'),el.getAttribute('title'),el.getAttribute('data-testid'),el.getAttribute('data-icon')].filter(Boolean).join(' '));
        const l=t.toLowerCase();let score=0;
        if(hint&&l.includes(hint))score+=160;
        for(const w of words)if(l.includes(w))score+=30;
        if(kind==='image'&&(l.includes('photo')||l.includes('image')||l.includes('media')))score+=80;
        if(kind==='video'&&(l.includes('video')||l.includes('media')))score+=80;
        if(kind==='document'&&(l.includes('document')||l.includes('file')))score+=70;
        return{{el,t,score,i}};
      }}).filter(x=>x.score>0).sort((a,b)=>b.score-a.score||a.i-b.i);
      if(!ranked.length)return{{ok:false}};
      const hit=ranked[0];hit.el.setAttribute('data-fatir-attach-tried','1');hit.el.scrollIntoView({{block:'center',inline:'center'}});const r=hit.el.getBoundingClientRect();
      return{{ok:true,text:hit.t||'attachment',x:Math.round(r.left+r.width/2),y:Math.round(r.top+r.height/2),width:Math.round(r.width),height:Math.round(r.height)}};
    }})()"#);

    let mut attached=false;
    let mut chooser_used=false;
    let mut last_error=None::<String>;

    for round in 0..4 {
        events.clear();
        seq+=1;
        match cdp_send_collect(&mut socket,seq,"Runtime.evaluate",json!({"expression":input_expression,"returnByValue":false}),&mut events).await {
            Ok(found) => {
                if let Some(object_id)=found.pointer("/result/objectId").and_then(Value::as_str) {
                    seq+=1;
                    match cdp_send_collect(&mut socket,seq,"DOM.setFileInputFiles",json!({"files":[path_text.clone()],"objectId":object_id}),&mut events).await {
                        Ok(_) => { attached=true; break; }
                        Err(err) => {
                            last_error=Some(err.to_string());
                            // Reactive sites can replace the input between discovery and assignment. Re-resolve it instead of failing the task.
                            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                        }
                    }
                }
            }
            Err(err) => last_error=Some(err.to_string()),
        }
        if attached { break; }

        // No durable input yet. Reveal the site's attachment UI using the virtual pointer, while intercepting any native file chooser.
        events.clear();
        seq+=1;
        let launch=cdp_send_collect(&mut socket,seq,"Runtime.evaluate",json!({"expression":launcher_expression,"returnByValue":true}),&mut events).await?;
        let v=launch.pointer("/result/value").cloned().unwrap_or_else(||json!({}));
        if v.get("ok").and_then(Value::as_bool)!=Some(true) {
            if round>=1 { break; }
            tokio::time::sleep(std::time::Duration::from_millis(220)).await;
            continue;
        }
        let x=v.get("x").and_then(Value::as_i64).unwrap_or(0); let y=v.get("y").and_then(Value::as_i64).unwrap_or(0);
        let w=v.get("width").and_then(Value::as_i64).unwrap_or(1); let h=v.get("height").and_then(Value::as_i64).unwrap_or(1);
        let lab=v.get("text").and_then(Value::as_str).unwrap_or("attachment");
        let _=browser_cursor_move(x,y,&format!("Fatir · {}",lab.chars().take(24).collect::<String>()),w,h).await;
        seq+=1; let _=cdp_send_collect(&mut socket,seq,"Input.dispatchMouseEvent",json!({"type":"mousePressed","x":x,"y":y,"button":"left","clickCount":1}),&mut events).await;
        seq+=1; let _=cdp_send_collect(&mut socket,seq,"Input.dispatchMouseEvent",json!({"type":"mouseReleased","x":x,"y":y,"button":"left","clickCount":1}),&mut events).await;
        let _=cdp_drain_events(&mut socket,&mut events,6).await;

        if let Some(backend)=file_chooser_backend_node(&events) {
            seq+=1;
            match cdp_send_collect(&mut socket,seq,"DOM.setFileInputFiles",json!({"files":[path_text.clone()],"backendNodeId":backend}),&mut events).await {
                Ok(_) => { attached=true; chooser_used=true; break; }
                Err(err) => last_error=Some(err.to_string()),
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(360)).await;
    }

    if intercept {
        seq+=1; let _=cdp_send_collect(&mut socket,seq,"Page.setInterceptFileChooserDialog",json!({"enabled":false}),&mut events).await;
    }
    if !attached {
        return Err(anyhow!("Fatir could not find or safely intercept a webpage attachment target after rescanning the live page. {}",last_error.unwrap_or_else(||"The site may use a cross-origin or unsupported upload widget.".to_string())));
    }

    tokio::time::sleep(std::time::Duration::from_millis(420)).await;
    seq+=1;
    let filename_js=serde_json::to_string(&filename)?;
    let verify_expression=format!(r#"(() => {{const name={filename_js};const input=[...document.querySelectorAll('input[type=file]')].find(x=>x.files&&[...x.files].some(f=>f.name===name));const text=(document.body?.innerText||'').includes(name);return{{input_has_file:!!input,filename_visible:text,url:location.href,title:document.title}};}})()"#);
    let verify=cdp_send_collect(&mut socket,seq,"Runtime.evaluate",json!({"expression":verify_expression,"returnByValue":true}),&mut events).await.ok();
    let verification=verify.as_ref().and_then(|v|v.pointer("/result/value")).cloned().unwrap_or_else(||json!({"accepted_by_browser":true}));
    let _=browser_cursor_hide().await;

    Ok(serde_json::to_string_pretty(&json!({
        "attached":true,
        "path":canonical.display().to_string(),
        "filename":filename,
        "kind":kind,
        "native_file_chooser_opened":false,
        "file_chooser_intercepted":chooser_used,
        "physical_mouse_moved":false,
        "verification":verification,
        "note":"Generic web attachment completed atomically. Fatir rescans dynamic DOM, keeps DevTools object handles in one session, and intercepts chooser-backed inputs when needed."
    }))?)
}

async fn browser_click_element(element_id: &str) -> Result<String> {
    let mcp_uid=Regex::new(r"^[0-9]+_[0-9]+$")?;
    if mcp_uid.is_match(element_id) { return chrome_mcp_click_uid(element_id).await; }
    pointer::ensure_enabled()?;
    let re = Regex::new(r"^fatir-[0-9]{1,5}$")?;
    if !re.is_match(element_id) { return Err(anyhow!("Invalid browser element ID; call browser_elements again")); }
    let id = serde_json::to_string(element_id)?;
    let script = format!(r#"(() => {{
      const id={id};
      const clean=v=>(v||'').replace(/\s+/g,' ').trim();
      const visible=el=>{{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>1&&r.height>1&&s.visibility!=='hidden'&&s.display!=='none'&&r.bottom>=0&&r.right>=0&&r.top<=innerHeight&&r.left<=innerWidth;}};
      const textOf=el=>clean(el.innerText||el.getAttribute('aria-label')||el.getAttribute('title')||el.getAttribute('placeholder')||el.value);
      let el=document.querySelector('[data-fatir-id='+CSS.escape(id)+']');
      let recovered=false;
      const loc=window.__fatirLocators&&window.__fatirLocators[id];
      if(!el&&loc){{
        const all=[...document.querySelectorAll('a,button,input,textarea,select,[role="button"],[role="link"],[role="checkbox"],[role="radio"],[role="tab"],[role="menuitem"],[contenteditable="true"],[onclick]')].filter(visible);
        const ranked=all.map((x,i)=>{{let score=0;const t=textOf(x);if(x.tagName.toLowerCase()===loc.tag)score+=25;if((x.getAttribute('type')||'')===loc.type)score+=20;if((x.getAttribute('role')||'')===loc.role)score+=15;if(loc.text&&t===loc.text)score+=120;else if(loc.text&&t&&(t.includes(loc.text)||loc.text.includes(t)))score+=60;if(loc.aria&&clean(x.getAttribute('aria-label'))===loc.aria)score+=90;if(loc.name&&clean(x.getAttribute('name'))===loc.name)score+=60;if(loc.href&&x.href===loc.href)score+=70;return{{x,score,i}};}}).sort((a,b)=>b.score-a.score||a.i-b.i);
        if(ranked[0]&&ranked[0].score>=60){{el=ranked[0].x;el.setAttribute('data-fatir-id',id);recovered=true;}}
      }}
      if(!el) return {{ok:false,error:'Page changed and the control could not be recovered. Inspect elements again.'}};
      el.scrollIntoView({{block:'center',inline:'center'}}); const r=el.getBoundingClientRect();
      return {{ok:true,recovered,text:textOf(el).slice(0,160),x:Math.round(r.left+r.width/2),y:Math.round(r.top+r.height/2),width:Math.round(r.width),height:Math.round(r.height)}};
    }})()"#);
    let out = cdp_command("Runtime.evaluate", json!({"expression":script,"returnByValue":true})).await?;
    let value = out.pointer("/result/value").cloned().unwrap_or_else(|| json!({}));
    if value.get("ok").and_then(Value::as_bool) != Some(true) { return Err(anyhow!("{}", value.get("error").and_then(Value::as_str).unwrap_or("Could not activate webpage element"))); }
    let x=value.get("x").and_then(Value::as_i64).ok_or_else(||anyhow!("Browser element has no x coordinate"))?;
    let y=value.get("y").and_then(Value::as_i64).ok_or_else(||anyhow!("Browser element has no y coordinate"))?;
    let w=value.get("width").and_then(Value::as_i64).unwrap_or(1); let h=value.get("height").and_then(Value::as_i64).unwrap_or(1);
    let label=value.get("text").and_then(Value::as_str).filter(|v|!v.is_empty()).unwrap_or(element_id);
    browser_virtual_click(x,y,&format!("Fatir · {}",label.chars().take(26).collect::<String>()),w,h).await?;
    Ok(serde_json::to_string_pretty(&json!({"activated":element_id,"label":label,"recovered_after_dom_change":value.get("recovered").and_then(Value::as_bool).unwrap_or(false),"virtual_pointer":true,"physical_mouse_moved":false}))?)
}

async fn browser_fill_element(element_id: &str, text: &str) -> Result<String> {
    if text.chars().count() > 20_000 { return Err(anyhow!("Text is too long for browser input")); }
    let mcp_uid=Regex::new(r"^[0-9]+_[0-9]+$")?;
    if mcp_uid.is_match(element_id) { return chrome_mcp_fill_uid(element_id,text).await; }
    pointer::ensure_enabled()?;
    let re = Regex::new(r"^fatir-[0-9]{1,5}$")?;
    if !re.is_match(element_id) { return Err(anyhow!("Invalid browser element ID; call browser_elements again")); }
    let id = serde_json::to_string(element_id)?;
    let script = format!(r#"(() => {{
      const id={id};const clean=v=>(v||'').replace(/\s+/g,' ').trim();
      const visible=el=>{{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>1&&r.height>1&&s.visibility!=='hidden'&&s.display!=='none';}};
      const labelOf=el=>clean(el.getAttribute('aria-label')||el.getAttribute('placeholder')||el.getAttribute('name')||el.innerText||'field');
      let el=document.querySelector('[data-fatir-id='+CSS.escape(id)+']');let recovered=false;const loc=window.__fatirLocators&&window.__fatirLocators[id];
      if(!el&&loc){{const fields=[...document.querySelectorAll('input:not([type="hidden"]),textarea,[contenteditable="true"]')].filter(visible);const ranked=fields.map((x,i)=>{{let score=0;const t=labelOf(x);if(x.tagName.toLowerCase()===loc.tag)score+=25;if((x.getAttribute('type')||'')===loc.type)score+=20;if(loc.text&&t===loc.text)score+=110;if(loc.placeholder&&clean(x.getAttribute('placeholder'))===loc.placeholder)score+=90;if(loc.name&&clean(x.getAttribute('name'))===loc.name)score+=70;return{{x,score,i}};}}).sort((a,b)=>b.score-a.score||a.i-b.i);if(ranked[0]&&ranked[0].score>=55){{el=ranked[0].x;el.setAttribute('data-fatir-id',id);recovered=true;}}}}
      if(!el) return {{ok:false,error:'Page changed and the field could not be recovered. Inspect elements again.'}};
      el.scrollIntoView({{block:'center',inline:'center'}}); const r=el.getBoundingClientRect();
      return {{ok:true,recovered,text:labelOf(el).slice(0,120),x:Math.round(r.left+r.width/2),y:Math.round(r.top+r.height/2),width:Math.round(r.width),height:Math.round(r.height)}};
    }})()"#);
    let out = cdp_command("Runtime.evaluate", json!({"expression":script,"returnByValue":true})).await?;
    let v=out.pointer("/result/value").cloned().unwrap_or_else(||json!({}));
    if v.get("ok").and_then(Value::as_bool)!=Some(true){return Err(anyhow!("{}",v.get("error").and_then(Value::as_str).unwrap_or("Could not find webpage field")));}
    let x=v.get("x").and_then(Value::as_i64).ok_or_else(||anyhow!("Browser field has no x coordinate"))?;
    let y=v.get("y").and_then(Value::as_i64).ok_or_else(||anyhow!("Browser field has no y coordinate"))?;
    let w=v.get("width").and_then(Value::as_i64).unwrap_or(1);let h=v.get("height").and_then(Value::as_i64).unwrap_or(1);
    let label=v.get("text").and_then(Value::as_str).unwrap_or("field");
    browser_virtual_click(x,y,&format!("Fatir · {}",label.chars().take(24).collect::<String>()),w,h).await?;
    browser_key("ctrl+a").await?; browser_key("Backspace").await?; browser_type(text).await?;
    Ok(serde_json::to_string_pretty(&json!({"filled":element_id,"characters":text.chars().count(),"recovered_after_dom_change":v.get("recovered").and_then(Value::as_bool).unwrap_or(false),"virtual_pointer":true,"physical_mouse_moved":false}))?)
}

async fn browser_click(x: i64, y: i64) -> Result<String> {
    pointer::ensure_enabled()?;
    let (width,height,_,_) = browser_viewport().await?;
    if x < 0 || y < 0 || x >= width || y >= height { return Err(anyhow!("Click coordinates ({x},{y}) are outside the webpage viewport {width}x{height}. Observe the browser again first.")); }
    browser_virtual_click(x,y,"Fatir · Click",0,0).await?;
    Ok(format!("Clicked webpage at ({x}, {y}) with Fatir's visible virtual pointer. The user's physical mouse was not moved."))
}

async fn browser_type(text: &str) -> Result<String> {
    if text.chars().count() > 20_000 { return Err(anyhow!("Text is too long for browser typing")); }
    if let Ok(result)=chrome_mcp_type(text).await { return Ok(result); }
    pointer::ensure_enabled()?;
    cdp_command("Input.insertText", json!({"text":text})).await?;
    Ok(format!("Inserted {} character(s) into the focused webpage field without using the physical keyboard.", text.chars().count()))
}

async fn browser_key(key: &str) -> Result<String> {
    if let Ok(result)=chrome_mcp_key(key).await { return Ok(result); }
    pointer::ensure_enabled()?;
    let re = Regex::new(r"^[A-Za-z0-9_+:-]{1,80}$")?;
    if !re.is_match(key) { return Err(anyhow!("Unsupported browser key sequence")); }
    let parts: Vec<&str> = key.split('+').collect();
    let raw = parts.last().copied().unwrap_or(key);
    let mut modifiers = 0u8;
    for part in &parts[..parts.len().saturating_sub(1)] {
        match part.to_ascii_lowercase().as_str() { "alt" => modifiers |= 1, "ctrl" | "control" => modifiers |= 2, "meta" | "super" => modifiers |= 4, "shift" => modifiers |= 8, _ => {} }
    }
    let k = match raw.to_ascii_lowercase().as_str() {
        "return" | "enter" => "Enter", "escape" | "esc" => "Escape", "tab" => "Tab", "backspace" => "Backspace",
        "left" => "ArrowLeft", "right" => "ArrowRight", "up" => "ArrowUp", "down" => "ArrowDown", "space" => " ", _ => raw,
    };
    cdp_command("Input.dispatchKeyEvent", json!({"type":"keyDown","key":k,"modifiers":modifiers})).await?;
    cdp_command("Input.dispatchKeyEvent", json!({"type":"keyUp","key":k,"modifiers":modifiers})).await?;
    tokio::time::sleep(std::time::Duration::from_millis(180)).await;
    Ok(format!("Sent {key} to the webpage through browser automation."))
}

async fn browser_scroll(direction: &str, steps: usize) -> Result<String> {
    pointer::ensure_enabled()?;
    let (width,height,_,_) = browser_viewport().await?;
    let amount = (steps.clamp(1,20) as i64) * 260;
    let delta = match direction { "up" => -amount, "down" => amount, _ => return Err(anyhow!("direction must be up or down")) };
    browser_cursor_move((width-54).max(20),height/2,&format!("Fatir · Scroll {direction}"),0,0).await?;
    cdp_command("Input.dispatchMouseEvent", json!({"type":"mouseWheel","x":(width-54).max(20),"y":height/2,"deltaX":0,"deltaY":delta})).await?;
    tokio::time::sleep(std::time::Duration::from_millis(280)).await;
    Ok(format!("Scrolled webpage {direction} {} step(s) with Fatir's virtual pointer; the physical mouse was not moved.", steps.clamp(1,20)))
}

async fn browser_open_url(value: &str) -> Result<String> {
    pointer::ensure_enabled()?;
    let parsed = url::Url::parse(value).context("Invalid URL")?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" { return Err(anyhow!("Only http and https URLs are allowed")); }
    ensure_fatir_browser().await?;
    if let Ok(result)=chrome_mcp_open_url(value).await { return Ok(result); }
    let _=browser_cursor_status("Fatir · Opening page").await;
    cdp_command("Page.navigate", json!({"url":value})).await?;
    wait_browser_ready(8000).await?;
    let _=browser_cursor_status("Fatir · Ready").await;
    Ok(format!("Opened {value} in Fatir's controlled browser. Fatir uses its own visible virtual pointer; the user's physical mouse remains free."))
}

async fn browser_get_url() -> Result<String> {
    if let Ok(url)=chrome_mcp_current_url().await { return Ok(url); }
    let target = browser_target().await?;
    let value = target.get("url").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if value.is_empty() { return Err(anyhow!("Browser URL was empty")); }
    Ok(value)
}

async fn web_search(query: &str, api_key: Option<&str>) -> Result<String> {
    let key = api_key.ok_or_else(|| anyhow!("Ollama Cloud API key is required for web search"))?;
    let res = Client::new().post("https://ollama.com/api/web_search").bearer_auth(key).json(&json!({"query":query})).send().await?.error_for_status()?.text().await?;
    Ok(shorten(&res, 100_000))
}

async fn download_file(url: &str, filename: &str) -> Result<String> {
    let safe = sanitize_filename(filename)?;
    let dir = dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads")).join("Fatir");
    fs::create_dir_all(&dir)?;
    let path = dir.join(safe);
    let _ = rollback::snapshot_file(&path, &format!("Undo download overwrite: {}", path.display()));
    let bytes = Client::new().get(url).send().await?.error_for_status()?.bytes().await?;
    tokio::fs::write(&path, &bytes).await?;
    Ok(format!("Downloaded {} to {}", human_size(bytes.len() as u64), path.display()))
}


fn write_text_file(path: &str, content: &str, create_parents: bool) -> Result<String> {
    let p=expand_path(path);
    if create_parents { if let Some(parent)=p.parent(){fs::create_dir_all(parent)?;} }
    let _=rollback::snapshot_file(&p,&format!("Restore file {}",p.display()))?;
    fs::write(&p,content)?;
    Ok(format!("Wrote {} bytes to {}",content.len(),p.display()))
}
fn copy_file(source:&str,destination:&str)->Result<String>{
    let src=expand_path(source).canonicalize()?; if !src.is_file(){return Err(anyhow!("Source is not a file"));}
    let dst=expand_path(destination); if let Some(parent)=dst.parent(){fs::create_dir_all(parent)?;}
    let _=rollback::snapshot_file(&dst,&format!("Restore destination {}",dst.display()))?;
    let bytes=fs::copy(&src,&dst)?; Ok(format!("Copied {} to {} ({} bytes)",src.display(),dst.display(),bytes))
}
fn move_path(source:&str,destination:&str)->Result<String>{
    let src=expand_path(source).canonicalize()?; let dst=expand_path(destination);
    if dst.exists(){return Err(anyhow!("Destination already exists; refusing to overwrite"));}
    if let Some(parent)=dst.parent(){fs::create_dir_all(parent)?;}
    fs::rename(&src,&dst)?;
    let _=rollback::record(&format!("Move {} back",dst.display()),"move_back",json!({"from":dst.display().to_string(),"to":src.display().to_string()}));
    Ok(format!("Moved {} to {}",src.display(),dst.display()))
}
fn create_directory(path:&str)->Result<String>{
    let p=expand_path(path); let existed=p.exists(); fs::create_dir_all(&p)?;
    if !existed{let _=rollback::record(&format!("Remove created folder {}",p.display()),"remove_created",json!({"path":p.display().to_string()}));}
    Ok(format!("Directory ready: {}",p.display()))
}

fn validate_shell_command(command: &str) -> Result<()> {
    if command.trim().is_empty() { return Err(anyhow!("Command cannot be empty")); }
    if command.len() > 12_000 { return Err(anyhow!("Command is too long")); }
    if command.contains('\0') { return Err(anyhow!("Command contains an invalid null byte")); }
    let lower = command.to_lowercase();
    if lower.contains("sudo ") || lower.starts_with("sudo") || lower.contains("pkexec ") || lower.starts_with("pkexec") {
        return Err(anyhow!("Do not use sudo or pkexec inside ordinary Fatir shell commands. Use run_privileged_command so Linux can show the PolicyKit approval dialog."));
    }
    Ok(())
}

async fn run_shell_command(command: &str, cwd: Option<&str>, timeout_seconds: u64) -> Result<String> {
    validate_shell_command(command)?;
    let mut cmd = Command::new("bash");
    cmd.arg("-lc").arg(command).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = cwd {
        let path = expand_path(dir);
        if !path.is_dir() { return Err(anyhow!("Working directory does not exist: {}", path.display())); }
        cmd.current_dir(path);
    }
    let timeout = timeout_seconds.clamp(1, 900);
    let output = tokio::time::timeout(std::time::Duration::from_secs(timeout), cmd.output())
        .await.map_err(|_| anyhow!("Command timed out after {timeout} seconds"))??;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(format!("Exit code: {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code().map(|v| v.to_string()).unwrap_or_else(|| "signal".into()),
        shorten(&stdout, 45_000), shorten(&stderr, 25_000)))
}


async fn run_shell_with_credentials(command: &str, cwd: Option<&str>, credential_map: &serde_json::Map<String, Value>, timeout_seconds: u64) -> Result<String> {
    validate_shell_command(command)?;
    let mut cmd = Command::new("bash");
    cmd.arg("-lc").arg(command).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = cwd {
        let path = expand_path(dir);
        if !path.is_dir() { return Err(anyhow!("Working directory does not exist: {}", path.display())); }
        cmd.current_dir(path);
    }
    let env_name = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$")?;
    let mut secrets = Vec::new();
    for (name, idv) in credential_map {
        if !env_name.is_match(name) { return Err(anyhow!("Invalid environment variable name: {name}")); }
        let id = idv.as_str().ok_or_else(|| anyhow!("Credential id for {name} must be a string"))?;
        let secret = credentials::secret(id)?;
        cmd.env(name, &secret);
        secrets.push(secret);
    }
    let timeout = timeout_seconds.clamp(1, 900);
    let output = tokio::time::timeout(std::time::Duration::from_secs(timeout), cmd.output())
        .await.map_err(|_| anyhow!("Command timed out after {timeout} seconds"))??;
    let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
    for secret in secrets {
        if !secret.is_empty() { stdout = stdout.replace(&secret, "[secret]"); stderr = stderr.replace(&secret, "[secret]"); }
    }
    Ok(format!("Exit code: {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code().map(|v| v.to_string()).unwrap_or_else(|| "signal".into()),
        shorten(&stdout,45_000), shorten(&stderr,25_000)))
}

async fn desktop_fill_credential(window: &str, path: &str, credential_id: &str) -> Result<String> {
    let secret = credentials::secret(credential_id)?;
    let result = desktop::set_secret_text(window, path, &secret).await?;
    drop(secret);
    Ok(serde_json::to_string_pretty(&json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(false),"window":window,"path":path,"credential_used":credential_id,"secret":"[not exposed]"}))?)
}

async fn browser_fill_credential(element_id: &str, credential_id: &str) -> Result<String> {
    // Stored secrets must never be placed on a subprocess command line or in model/MCP context.
    // Resolve an MCP uid to its accessible label, then fill through Fatir's in-process CDP channel.
    let secret = credentials::secret(credential_id)?;
    let mcp_uid=Regex::new(r"^[0-9]+_[0-9]+$")?;
    let label = if mcp_uid.is_match(element_id) {
        let (_,snapshot)=chrome_mcp_snapshot().await?;
        chrome_mcp_label_for_uid(&snapshot,element_id)
    } else { None };
    let id_js=serde_json::to_string(element_id)?;
    let label_js=serde_json::to_string(label.as_deref().unwrap_or(""))?;
    let script=format!(r#"(() => {{
      const id={id_js}, wanted={label_js}.toLowerCase().replace(/\s+/g,' ').trim();
      const clean=s=>(s||'').replace(/\s+/g,' ').trim();
      const visible=el=>{{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>1&&r.height>1&&s.visibility!=='hidden'&&s.display!=='none';}};
      const labelFor=el=>{{let a=[el.getAttribute('aria-label'),el.getAttribute('placeholder'),el.getAttribute('name'),el.getAttribute('id')].filter(Boolean).join(' ');if(el.id){{const l=document.querySelector('label[for="'+CSS.escape(el.id)+'"]');if(l)a+=' '+(l.innerText||'');}}const parent=el.closest('label');if(parent)a+=' '+(parent.innerText||'');return clean(a);}};
      let el=id.startsWith('fatir-')?document.querySelector('[data-fatir-id="'+CSS.escape(id)+'"]'):null;
      if(!el&&wanted){{const fields=[...document.querySelectorAll('input:not([type="hidden"]),textarea,[contenteditable="true"]')].filter(visible);const ranked=fields.map((x,i)=>{{const t=labelFor(x),l=t.toLowerCase();let score=l===wanted?120:l.includes(wanted)?80:wanted.includes(l)&&l.length>2?45:0;return{{x,t,score,i}};}}).filter(x=>x.score>0).sort((a,b)=>b.score-a.score||a.i-b.i);if(ranked[0])el=ranked[0].x;}}
      if(!el)return{{ok:false,error:'Could not safely resolve credential field through CDP'}};
      el.scrollIntoView({{block:'center',inline:'center'}}); el.focus();
      return{{ok:true,label:labelFor(el)}};
    }})()"#);
    let out=cdp_command("Runtime.evaluate",json!({"expression":script,"returnByValue":true})).await?;
    let v=out.pointer("/result/value").cloned().unwrap_or_else(||json!({}));
    if v.get("ok").and_then(Value::as_bool)!=Some(true){return Err(anyhow!("{}",v.get("error").and_then(Value::as_str).unwrap_or("Could not safely resolve credential field")));}
    cdp_command("Input.dispatchKeyEvent",json!({"type":"keyDown","key":"a","code":"KeyA","modifiers":2})).await?;
    cdp_command("Input.dispatchKeyEvent",json!({"type":"keyUp","key":"a","code":"KeyA","modifiers":2})).await?;
    cdp_command("Input.dispatchKeyEvent",json!({"type":"keyDown","key":"Backspace"})).await?;
    cdp_command("Input.dispatchKeyEvent",json!({"type":"keyUp","key":"Backspace"})).await?;
    cdp_command("Input.insertText",json!({"text":secret})).await?;
    drop(secret);
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"element_id":element_id,"credential_used":credential_id,"secret":"[not exposed]","engine":"direct-cdp-secret-safe"}))?)
}

async fn browser_fill_credential_by_label(label: &str, credential_id: &str) -> Result<String> {
    if label.trim().is_empty() { return Err(anyhow!("Credential field label cannot be empty")); }
    // Keep the secret entirely inside Fatir. The model only provides a semantic field label
    // and credential id; the secret never enters model context, MCP arguments, logs or history.
    ensure_fatir_browser().await?;
    let secret = credentials::secret(credential_id)?;
    let wanted=serde_json::to_string(label.trim())?;
    let script=format!(r#"(() => {{
      const wanted={wanted}.toLowerCase().replace(/\s+/g,' ').trim();
      const clean=s=>(s||'').replace(/\s+/g,' ').trim();
      const visible=el=>{{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>1&&r.height>1&&s.visibility!=='hidden'&&s.display!=='none';}};
      const labelFor=el=>{{let a=[el.getAttribute('aria-label'),el.getAttribute('placeholder'),el.getAttribute('name'),el.getAttribute('id'),el.getAttribute('autocomplete')].filter(Boolean).join(' ');if(el.id){{const l=document.querySelector('label[for="'+CSS.escape(el.id)+'"]');if(l)a+=' '+(l.innerText||'');}}const p=el.closest('label');if(p)a+=' '+(p.innerText||'');return clean(a);}};
      const fields=[...document.querySelectorAll('input:not([type="hidden"]),textarea,[contenteditable="true"]')].filter(visible);
      const ranked=fields.map((el,i)=>{{const t=labelFor(el),l=t.toLowerCase();let score=l===wanted?140:l.includes(wanted)?100:wanted.includes(l)&&l.length>2?60:0;if(wanted.includes('password')&&el.type==='password')score+=80;if((wanted.includes('email')||wanted.includes('user'))&&/email|username/.test((el.autocomplete||'')+' '+(el.type||'')))score+=40;return{{el,t,score,i}};}}).filter(x=>x.score>0).sort((a,b)=>b.score-a.score||a.i-b.i);
      if(!ranked.length)return{{ok:false,error:'No visible credential field matched',query:wanted}};
      const el=ranked[0].el;el.scrollIntoView({{block:'center',inline:'center'}});el.focus();
      return{{ok:true,label:ranked[0].t||wanted,type:el.type||''}};
    }})()"#);
    let out=cdp_command("Runtime.evaluate",json!({"expression":script,"returnByValue":true})).await?;
    let v=out.pointer("/result/value").cloned().unwrap_or_else(||json!({}));
    if v.get("ok").and_then(Value::as_bool)!=Some(true){return Err(anyhow!("{}",v.get("error").and_then(Value::as_str).unwrap_or("Could not safely resolve credential field")));}
    cdp_command("Input.dispatchKeyEvent",json!({"type":"keyDown","key":"a","code":"KeyA","modifiers":2})).await?;
    cdp_command("Input.dispatchKeyEvent",json!({"type":"keyUp","key":"a","code":"KeyA","modifiers":2})).await?;
    cdp_command("Input.dispatchKeyEvent",json!({"type":"keyDown","key":"Backspace"})).await?;
    cdp_command("Input.dispatchKeyEvent",json!({"type":"keyUp","key":"Backspace"})).await?;
    cdp_command("Input.insertText",json!({"text":secret})).await?;
    drop(secret);
    Ok(serde_json::to_string_pretty(&json!({"ok":true,"field":label,"credential_used":credential_id,"secret":"[not exposed]","engine":"direct-cdp-secret-safe"}))?)
}

async fn run_privileged_command(command: &str, timeout_seconds: u64) -> Result<String> {
    validate_shell_command(command)?;
    let timeout = timeout_seconds.clamp(1, 900);
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout),
        Command::new("pkexec").arg("bash").arg("-lc").arg(command)
            .stdout(Stdio::piped()).stderr(Stdio::piped()).output()
    ).await.map_err(|_| anyhow!("Privileged command timed out after {timeout} seconds"))??;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(anyhow!("Privileged command exited with {}. stdout: {} stderr: {}",
            output.status.code().map(|v| v.to_string()).unwrap_or_else(|| "signal".into()),
            shorten(&stdout, 12_000), shorten(&stderr, 12_000)));
    }
    Ok(format!("Exit code: 0\n--- stdout ---\n{}\n--- stderr ---\n{}", shorten(&stdout,45_000), shorten(&stderr,25_000)))
}

async fn install_apt(package: &str) -> Result<String> {
    validate_token(package)?;
    let was_installed = Command::new("dpkg-query").args(["-W", "-f=${Status}", package]).output().await.ok()
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("install ok installed")).unwrap_or(false);
    let status = Command::new("pkexec").args(["apt-get", "install", "-y", package]).status().await?;
    if !status.success() { return Err(anyhow!("APT installation failed")); }
    if !was_installed { let _ = rollback::record(&format!("Remove newly installed APT package {package}"), "apt_remove", json!({"package":package})); }
    Ok(format!("Installed APT package {package}{}", if was_installed { " (already present; verified)" } else { "" }))
}

async fn install_flatpak(app_id: &str) -> Result<String> {
    validate_token(app_id)?;
    let was_installed = Command::new("flatpak").args(["info", "--user", app_id]).status().await.map(|s|s.success()).unwrap_or(false);
    let status = Command::new("flatpak").args(["install", "--user", "-y", "flathub", app_id]).status().await?;
    if !status.success() { return Err(anyhow!("Flatpak installation failed")); }
    if !was_installed { let _ = rollback::record(&format!("Remove newly installed Flatpak {app_id}"), "flatpak_remove", json!({"app_id":app_id})); }
    Ok(format!("Installed Flatpak {app_id}{}", if was_installed { " (already present; verified)" } else { "" }))
}

async fn install_deb(path: &str) -> Result<String> {
    let p = expand_path(path).canonicalize()?;
    if p.extension().and_then(|x| x.to_str()) != Some("deb") { return Err(anyhow!("Not a .deb package")); }
    let pkg_out = Command::new("dpkg-deb").args(["-f", p.to_string_lossy().as_ref(), "Package"]).output().await.ok();
    let package = pkg_out.as_ref().filter(|o|o.status.success()).map(|o|String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|x|!x.is_empty());
    let was_installed = if let Some(pkg)=package.as_deref(){Command::new("dpkg-query").args(["-W", "-f=${Status}", pkg]).output().await.ok().map(|o|o.status.success()&&String::from_utf8_lossy(&o.stdout).contains("install ok installed")).unwrap_or(false)}else{false};
    let status = Command::new("pkexec").args(["apt-get", "install", "-y", p.to_string_lossy().as_ref()]).status().await?;
    if !status.success() { return Err(anyhow!(".deb installation failed")); }
    if !was_installed { if let Some(pkg)=package.as_deref(){let _=rollback::record(&format!("Remove newly installed package {pkg}"),"apt_remove",json!({"package":pkg}));} }
    Ok(format!("Installed {}", p.display()))
}

async fn extract_archive(archive: &str, destination: &str) -> Result<String> {
    let a = expand_path(archive).canonicalize()?;
    let d = expand_path(destination);
    if d.exists() && fs::read_dir(&d).map(|mut it|it.next().is_some()).unwrap_or(false) {
        return Err(anyhow!("Destination is not empty. Choose a new/empty folder so extraction cannot silently overwrite existing files."));
    }
    if !d.exists() { let _ = rollback::record(&format!("Remove extracted folder {}", d.display()), "remove_created", json!({"path":d.display().to_string()})); }
    fs::create_dir_all(&d)?;
    let name = a.to_string_lossy().to_lowercase();
    let status = if name.ends_with(".zip") {
        Command::new("unzip").arg("-o").arg(&a).arg("-d").arg(&d).status().await?
    } else {
        Command::new("tar").arg("-xf").arg(&a).arg("-C").arg(&d).status().await?
    };
    if !status.success() { return Err(anyhow!("Archive extraction failed")); }
    Ok(format!("Extracted {} to {}", a.display(), d.display()))
}

async fn create_desktop_entry(name: &str, exec: &str, icon: Option<&str>) -> Result<String> {
    if name.trim().is_empty() || exec.trim().is_empty() { return Err(anyhow!("Name and exec are required")); }
    let slug = name.to_lowercase().replace(|c: char| !c.is_ascii_alphanumeric(), "-").trim_matches('-').to_string();
    let dir = dirs::data_local_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share")).join("applications");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("ah-{slug}.desktop"));
    let _ = rollback::snapshot_file(&path, &format!("Restore launcher {}", path.display()));
    let content = format!("[Desktop Entry]\nType=Application\nName={}\nExec={}\nIcon={}\nTerminal=false\nCategories=Utility;\n", name.replace('\n', " "), exec.replace('\n', " "), icon.unwrap_or("application-x-executable").replace('\n', " "));
    fs::write(&path, content)?;
    let _ = Command::new("update-desktop-database").arg(&dir).status().await;
    Ok(format!("Created launcher {}", path.display()))
}

async fn move_to_trash(path: &str) -> Result<String> {
    let p = expand_path(path).canonicalize()?;
    let home = dirs::home_dir();
    if p == Path::new("/") || home.as_ref().map(|h| h == &p).unwrap_or(false) { return Err(anyhow!("Refusing to trash a protected root path")); }
    let status = Command::new("gio").arg("trash").arg(&p).status().await?;
    if !status.success() { return Err(anyhow!("Could not move item to Trash")); }
    Ok(format!("Moved {} to Trash", p.display()))
}

fn validate_token(s: &str) -> Result<()> {
    let re = Regex::new(r"^[A-Za-z0-9._:+-]+$")?;
    if !re.is_match(s) { return Err(anyhow!("Unsafe package identifier")); }
    Ok(())
}

fn sanitize_filename(s: &str) -> Result<String> {
    if s.contains('/') || s.contains('\\') || s == "." || s == ".." || s.trim().is_empty() { return Err(anyhow!("Unsafe filename")); }
    Ok(s.to_string())
}

fn which(bin: &str) -> bool {
    std::process::Command::new("sh").args(["-c", &format!("command -v {} >/dev/null 2>&1", bin)]).status().map(|s| s.success()).unwrap_or(false)
}

pub fn human_size(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut n = bytes as f64;
    let mut i = 0;
    while n >= 1024.0 && i < U.len() - 1 { n /= 1024.0; i += 1; }
    if i == 0 { format!("{} {}", bytes, U[i]) } else { format!("{:.1} {}", n, U[i]) }
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max).collect::<String>()) }
}

// --- Fatir Share browser adapters -------------------------------------------------
// These deterministic helpers are intentionally model-free. They reuse the same
// CDP session/cursor stack as ordinary Fatir browser control so local-file handoff
// remains visible, verifiable, and independent of the user's physical mouse.

pub(crate) async fn share_whatsapp_contacts_live() -> Result<Vec<String>> {
    pointer::ensure_enabled()?;
    ensure_fatir_browser().await?;
    let current = browser_get_url().await.unwrap_or_default();
    if !current.starts_with("https://web.whatsapp.com") {
        browser_open_url("https://web.whatsapp.com/").await?;
        tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
    }
    let script = r#"(() => {
      const body=(document.body?.innerText||'').toLowerCase();
      const pane=document.querySelector('#pane-side');
      const login=!pane && (body.includes('log in to whatsapp')||body.includes('link with phone number')||!!document.querySelector('canvas'));
      if(login)return {login:true,contacts:[]};
      if(!pane)return {login:false,contacts:[]};
      const seen=new Set(), out=[];
      const push=v=>{v=(v||'').replace(/\s+/g,' ').trim();if(!v||v.length>90||seen.has(v))return;
        if(/^\d{1,2}:\d{2}(\s?[ap]m)?$/i.test(v)||/^(yesterday|today)$/i.test(v))return;
        if(/^\+?[\d\s()-]{5,}$/.test(v) && v.replace(/\D/g,'').length<7)return;
        seen.add(v);out.push(v);
      };
      pane.querySelectorAll('span[title]').forEach(x=>push(x.getAttribute('title')));
      pane.querySelectorAll('[role="row"] span[dir="auto"], [role="listitem"] span[dir="auto"]').forEach(x=>push(x.textContent));
      return {login:false,contacts:out.slice(0,80)};
    })()"#;
    let out = cdp_command("Runtime.evaluate", json!({"expression":script,"returnByValue":true})).await?;
    let value = out.pointer("/result/value").cloned().unwrap_or_else(||json!({}));
    if value.get("login").and_then(Value::as_bool).unwrap_or(false) {
        return Err(anyhow!("WhatsApp Web needs you to sign in once in Fatir's controlled browser"));
    }
    let contacts=value.get("contacts").and_then(Value::as_array).cloned().unwrap_or_default()
        .into_iter().filter_map(|x|x.as_str().map(str::to_string)).collect::<Vec<_>>();
    Ok(contacts)
}

async fn whatsapp_open_contact(contact: &str) -> Result<String> {
    pointer::ensure_enabled()?;
    ensure_fatir_browser().await?;
    let current=browser_get_url().await.unwrap_or_default();
    if !current.starts_with("https://web.whatsapp.com") {
        browser_open_url("https://web.whatsapp.com/").await?;
        tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
    }
    let wanted=serde_json::to_string(contact.trim())?;
    // Prefer a currently visible chat. If absent, use WhatsApp's own search box,
    // then reacquire the result after the reactive list rerenders.
    let find_script=format!(r#"(() => {{
      const wanted={wanted}.toLowerCase(); const pane=document.querySelector('#pane-side');
      if(!pane)return null;
      const visible=el=>{{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>2&&r.height>2&&s.visibility!=='hidden'&&s.display!=='none';}};
      const els=[...pane.querySelectorAll('span[title],[role="row"],[role="listitem"]')].filter(visible);
      for(const el of els){{const t=((el.getAttribute('title')||el.innerText||'')+'').replace(/\s+/g,' ').trim();if(t.toLowerCase()===wanted||t.toLowerCase().startsWith(wanted)){{const r=el.getBoundingClientRect();return{{text:t,x:Math.round(r.left+r.width/2),y:Math.round(r.top+r.height/2),w:Math.round(r.width),h:Math.round(r.height)}};}}}}
      return null;
    }})()"#);
    let mut out=cdp_command("Runtime.evaluate",json!({"expression":find_script,"returnByValue":true})).await?;
    let mut hit=out.pointer("/result/value").cloned().unwrap_or(Value::Null);
    if hit.is_null() {
        let search_script=r#"(() => {
          const els=[...document.querySelectorAll('[contenteditable="true"][role="textbox"],input[type="text"]')];
          const el=els.find(x=>/search/i.test((x.getAttribute('aria-label')||x.getAttribute('title')||x.getAttribute('placeholder')||''))) || els[0];
          if(!el)return null; const r=el.getBoundingClientRect(); return {x:Math.round(r.left+r.width/2),y:Math.round(r.top+r.height/2),w:Math.round(r.width),h:Math.round(r.height)};
        })()"#;
        let sr=cdp_command("Runtime.evaluate",json!({"expression":search_script,"returnByValue":true})).await?;
        let v=sr.pointer("/result/value").cloned().unwrap_or(Value::Null);
        if v.is_null(){return Err(anyhow!("Could not find WhatsApp's chat search field"));}
        let x=v.get("x").and_then(Value::as_i64).unwrap_or(0);let y=v.get("y").and_then(Value::as_i64).unwrap_or(0);
        browser_virtual_click(x,y,"Fatir · Search WhatsApp",v.get("w").and_then(Value::as_i64).unwrap_or(0),v.get("h").and_then(Value::as_i64).unwrap_or(0)).await?;
        let _=browser_key("Ctrl+A").await; let _=browser_type(contact).await?;
        tokio::time::sleep(std::time::Duration::from_millis(950)).await;
        out=cdp_command("Runtime.evaluate",json!({"expression":find_script,"returnByValue":true})).await?;
        hit=out.pointer("/result/value").cloned().unwrap_or(Value::Null);
    }
    if hit.is_null(){return Err(anyhow!("Could not find WhatsApp contact '{contact}'. Try opening that chat once or use the Share Sheet search."));}
    let x=hit.get("x").and_then(Value::as_i64).unwrap_or(0);let y=hit.get("y").and_then(Value::as_i64).unwrap_or(0);
    let label=hit.get("text").and_then(Value::as_str).unwrap_or(contact).to_string();
    browser_virtual_click(x,y,&format!("Fatir · {label}"),hit.get("w").and_then(Value::as_i64).unwrap_or(0),hit.get("h").and_then(Value::as_i64).unwrap_or(0)).await?;
    tokio::time::sleep(std::time::Duration::from_millis(650)).await;
    Ok(label)
}

async fn whatsapp_message_snapshot(filename: &str) -> Result<Value> {
    let name_js=serde_json::to_string(filename)?;
    let script=format!(r#"(() => {{
      const name={name_js};
      const root=document.querySelector('#main')||document;
      const clean=v=>(v||'').replace(/\s+/g,' ').trim();
      const uniq=items=>[...new Set(items.filter(Boolean))];
      const outgoing=uniq([
        ...root.querySelectorAll('.message-out'),
        ...root.querySelectorAll('[data-id^="true_"]')
      ].map(el=>el.closest('.message-out')||el.closest('[data-id^="true_"]')||el));
      const all=uniq([
        ...root.querySelectorAll('.message-in,.message-out'),
        ...root.querySelectorAll('[data-testid="msg-container"]')
      ]);
      const last=outgoing[outgoing.length-1]||null;
      const lastId=last ? (last.getAttribute('data-id')||last.querySelector?.('[data-id]')?.getAttribute('data-id')||'') : '';
      const filenameMatches=outgoing.filter(el=>clean(el.innerText).includes(name)).length;
      return {{outgoing_count:outgoing.length,message_count:all.length,last_outgoing_id:lastId,filename_matches:filenameMatches}};
    }})()"#);
    let out=cdp_command("Runtime.evaluate",json!({"expression":script,"returnByValue":true})).await?;
    Ok(out.pointer("/result/value").cloned().unwrap_or_else(||json!({})))
}

async fn whatsapp_attachment_state(filename: &str, kind: &str) -> Result<Value> {
    let name_js=serde_json::to_string(filename)?;
    let kind_js=serde_json::to_string(kind)?;
    let script=format!(r#"(() => {{
      const name={name_js}, kind={kind_js};
      const clean=v=>(v||'').replace(/\s+/g,' ').trim();
      const visible=el=>{{if(!el)return false;const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>2&&r.height>2&&s.display!=='none'&&s.visibility!=='hidden'&&Number(s.opacity||1)>0.02&&r.bottom>=0&&r.right>=0&&r.top<=innerHeight&&r.left<=innerWidth;}};
      const captions=[...document.querySelectorAll('[contenteditable="true"],[role="textbox"],textarea,input[type="text"]')].filter(visible).filter(el=>/caption|add a caption/i.test(clean([el.getAttribute('aria-label'),el.getAttribute('placeholder'),el.getAttribute('title')].filter(Boolean).join(' '))));
      const markers=[...document.querySelectorAll('[role="dialog"],[data-testid*="preview" i],[data-testid*="media-editor" i],[data-testid*="media-preview" i],[aria-label*="preview" i]')].filter(visible);
      const media=[...document.querySelectorAll('img,video,canvas')].filter(visible).filter(el=>{{const r=el.getBoundingClientRect();return r.width>160&&r.height>110;}});
      const filenameVisible=[...document.querySelectorAll('body *')].filter(visible).some(el=>{{const t=clean(el.innerText);return t&&t.length<500&&t.includes(name);}});
      const fileInputs=[...document.querySelectorAll('input[type=file]')];
      const inputHasFile=fileInputs.some(x=>x.files&&[...x.files].some(f=>f.name===name));
      const previewActive=markers.length>0 || (captions.length>0 && (filenameVisible || media.length>0 || inputHasFile));
      return {{preview_active:previewActive,caption:captions.length>0,markers:markers.length,media:media.length,filename_visible:filenameVisible,input_has_file:inputHasFile,kind}};
    }})()"#);
    let out=cdp_command("Runtime.evaluate",json!({"expression":script,"returnByValue":true})).await?;
    Ok(out.pointer("/result/value").cloned().unwrap_or_else(||json!({})))
}

async fn whatsapp_wait_attachment_preview(filename: &str, kind: &str) -> Result<Value> {
    let mut last=json!({});
    for _ in 0..24 {
        last=whatsapp_attachment_state(filename,kind).await?;
        let active=last.get("preview_active").and_then(Value::as_bool).unwrap_or(false);
        let filename_visible=last.get("filename_visible").and_then(Value::as_bool).unwrap_or(false);
        let media=last.get("media").and_then(Value::as_u64).unwrap_or(0)>0;
        let input_has_file=last.get("input_has_file").and_then(Value::as_bool).unwrap_or(false);
        let accepted = if kind=="document" {
            active && input_has_file && filename_visible
        } else {
            // WhatsApp's image/video editor usually does not render the local filename.
            // For media, the populated file input + active visual preview is the proof.
            active && input_has_file && media
        };
        if accepted { return Ok(last); }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    Err(anyhow!("WhatsApp did not show a verified attachment preview for '{filename}'. Fatir stopped before Send instead of guessing. Last state: {}",last))
}

async fn whatsapp_click_send(filename: &str, kind: &str) -> Result<()> {
    pointer::ensure_enabled()?;
    let name_js=serde_json::to_string(filename)?;
    let kind_js=serde_json::to_string(kind)?;
    let locate_script=format!(r#"(() => {{
      document.querySelectorAll('[data-fatir-whatsapp-send]').forEach(x=>x.removeAttribute('data-fatir-whatsapp-send'));
      const name={name_js}, kind={kind_js};
      const clean=v=>(v||'').replace(/\s+/g,' ').trim();
      const visible=el=>{{if(!el)return false;const r=el.getBoundingClientRect(),s=getComputedStyle(el);return r.width>2&&r.height>2&&s.display!=='none'&&s.visibility!=='hidden'&&Number(s.opacity||1)>0.02&&r.bottom>=0&&r.right>=0&&r.top<=innerHeight&&r.left<=innerWidth;}};
      const clickRoot=el=>el.closest('button,[role="button"],[tabindex],[onclick]')||el;
      const captions=[...document.querySelectorAll('[contenteditable="true"],[role="textbox"],textarea,input[type="text"]')].filter(visible).filter(el=>/caption|add a caption/i.test(clean([el.getAttribute('aria-label'),el.getAttribute('placeholder'),el.getAttribute('title')].filter(Boolean).join(' '))));
      const markers=[...document.querySelectorAll('[role="dialog"],[data-testid*="preview" i],[data-testid*="media-editor" i],[data-testid*="media-preview" i],[aria-label*="preview" i]')].filter(visible);
      const media=[...document.querySelectorAll('img,video,canvas')].filter(visible).some(el=>{{const r=el.getBoundingClientRect();return r.width>160&&r.height>110;}});
      const filenameVisible=[...document.querySelectorAll('body *')].filter(visible).some(el=>{{const t=clean(el.innerText);return t&&t.length<500&&t.includes(name);}});
      const inputHasFile=[...document.querySelectorAll('input[type=file]')].some(x=>x.files&&[...x.files].some(f=>f.name===name));
      const previewActive=markers.length>0 || (captions.length>0 && (filenameVisible || media || inputHasFile));
      const previewVerified=kind==='document'
        ? (previewActive && inputHasFile && filenameVisible)
        : (previewActive && inputHasFile && media);
      if(!previewVerified)return {{ok:false,error:'attachment_preview_not_verified',previewActive,inputHasFile,filenameVisible,media,kind}};
      const raw=[...document.querySelectorAll('button,[role="button"],[aria-label],[title],[data-testid],[data-icon],span[data-icon],svg')].filter(visible);
      const seen=new Set(), hits=[];
      const blacklisted=/\b(forward|share|reply|delete|cancel|remove|next)\b/i;
      for(const src of raw){{
        const el=clickRoot(src); if(!visible(el)||seen.has(el))continue; seen.add(el);
        const values=[
          src.getAttribute('aria-label'),src.getAttribute('title'),src.getAttribute('data-testid'),src.getAttribute('data-icon'),
          el.getAttribute('aria-label'),el.getAttribute('title'),el.getAttribute('data-testid'),el.getAttribute('data-icon'),
          el.querySelector?.('[data-icon]')?.getAttribute('data-icon'),el.querySelector?.('[aria-label]')?.getAttribute('aria-label')
        ].filter(Boolean).map(clean);
        const joined=values.join(' ');
        if(blacklisted.test(joined))continue;
        const semantic=values.some(v=>v.toLowerCase()==='send'||/(^|[-_])send($|[-_])/i.test(v));
        if(!semantic)continue;
        if(el.disabled||el.getAttribute('aria-disabled')==='true')continue;
        const r=el.getBoundingClientRect();
        const inMarker=markers.some(m=>m===el||m.contains(el));
        const nearCaption=captions.some(c=>{{const cr=c.getBoundingClientRect();return r.top>=cr.top-180&&r.left>=cr.left-220;}});
        if(markers.length>0&&!inMarker&&!nearCaption)continue;
        let score=1000;
        if(inMarker)score+=300;
        if(nearCaption)score+=180;
        if(r.left>innerWidth*.55)score+=40;
        if(r.top>innerHeight*.45)score+=30;
        hits.push({{el,score,label:joined,r}});
      }}
      hits.sort((a,b)=>b.score-a.score);
      if(!hits.length)return {{ok:false,error:'verified_send_not_found'}};
      const h=hits[0],r=h.el.getBoundingClientRect();
      h.el.setAttribute('data-fatir-whatsapp-send','1');
      return {{ok:true,x:Math.round(r.left+r.width/2),y:Math.round(r.top+r.height/2),w:Math.round(r.width),h:Math.round(r.height),label:h.label}};
    }})()"#);

    for _ in 0..12 {
        let out=cdp_command("Runtime.evaluate",json!({"expression":locate_script,"returnByValue":true})).await?;
        let v=out.pointer("/result/value").cloned().unwrap_or_else(||json!({}));
        if v.get("ok").and_then(Value::as_bool)==Some(true) {
            let x=v.get("x").and_then(Value::as_i64).unwrap_or(0);
            let y=v.get("y").and_then(Value::as_i64).unwrap_or(0);
            let w=v.get("w").and_then(Value::as_i64).unwrap_or(1);
            let h=v.get("h").and_then(Value::as_i64).unwrap_or(1);
            let _=browser_cursor_move(x,y,"Fatir · Send",w,h).await;
            let click_script=r#"(() => {
              const el=document.querySelector('[data-fatir-whatsapp-send="1"]');
              if(!el)return {ok:false,error:'send_target_replaced'};
              const clean=v=>(v||'').replace(/\s+/g,' ').trim();
              const values=[el.getAttribute('aria-label'),el.getAttribute('title'),el.getAttribute('data-testid'),el.getAttribute('data-icon'),el.querySelector?.('[data-icon]')?.getAttribute('data-icon')].filter(Boolean).map(clean);
              const joined=values.join(' ');
              if(/\b(forward|share|reply|delete|cancel|remove|next)\b/i.test(joined))return {ok:false,error:'unsafe_target_rejected'};
              if(!values.some(v=>v.toLowerCase()==='send'||/(^|[-_])send($|[-_])/i.test(v)))return {ok:false,error:'target_no_longer_send'};
              el.removeAttribute('data-fatir-whatsapp-send');
              el.dispatchEvent(new MouseEvent('mousedown',{bubbles:true,cancelable:true,view:window}));
              el.dispatchEvent(new MouseEvent('mouseup',{bubbles:true,cancelable:true,view:window}));
              el.click();
              return {ok:true};
            })()"#;
            let clicked=cdp_command("Runtime.evaluate",json!({"expression":click_script,"returnByValue":true})).await?;
            if clicked.pointer("/result/value/ok").and_then(Value::as_bool)==Some(true) {
                tokio::time::sleep(std::time::Duration::from_millis(450)).await;
                return Ok(());
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(220)).await;
    }
    Err(anyhow!("Verified WhatsApp Send button not found inside the active attachment preview. Fatir refused to click Forward or another guessed control."))
}

async fn whatsapp_wait_sent(filename: &str, kind: &str, baseline: &Value) -> Result<Value> {
    let before_out=baseline.get("outgoing_count").and_then(Value::as_u64).unwrap_or(0);
    let before_msg=baseline.get("message_count").and_then(Value::as_u64).unwrap_or(0);
    let before_matches=baseline.get("filename_matches").and_then(Value::as_u64).unwrap_or(0);
    let before_last=baseline.get("last_outgoing_id").and_then(Value::as_str).unwrap_or("").to_string();
    let mut last=json!({});
    for _ in 0..28 {
        let snap=whatsapp_message_snapshot(filename).await?;
        let preview=whatsapp_attachment_state(filename,kind).await.unwrap_or_else(|_|json!({}));
        let preview_active=preview.get("preview_active").and_then(Value::as_bool).unwrap_or(false);
        let out=snap.get("outgoing_count").and_then(Value::as_u64).unwrap_or(0);
        let msg=snap.get("message_count").and_then(Value::as_u64).unwrap_or(0);
        let matches=snap.get("filename_matches").and_then(Value::as_u64).unwrap_or(0);
        let last_id=snap.get("last_outgoing_id").and_then(Value::as_str).unwrap_or("");
        let new_outgoing=out>before_out || msg>before_msg || matches>before_matches || (!last_id.is_empty()&&last_id!=before_last);
        last=json!({"snapshot":snap,"preview":preview,"new_outgoing":new_outgoing});
        if !preview_active && new_outgoing { return Ok(last); }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    Err(anyhow!("WhatsApp Send was activated, but Fatir could not verify that '{filename}' appeared as a new outgoing message. The task was not marked complete. Last state: {}",last))
}

pub(crate) async fn share_whatsapp_send_files(paths: &[PathBuf], contact: &str) -> Result<Value> {
    let result: Result<Value> = async {
        let opened=whatsapp_open_contact(contact).await?;
        eprintln!("[Fatir WhatsApp] CHAT_CONFIRMED {}",opened);
        let mut sent=Vec::new();
        for path in paths {
            let canonical=fs::canonicalize(path).with_context(||format!("Could not resolve {}",path.display()))?;
            if !canonical.is_file(){return Err(anyhow!("Requested share path is not a regular file: {}",canonical.display()));}
            let filename=canonical.file_name().and_then(|x|x.to_str()).unwrap_or("file").to_string();
            let ext=canonical.extension().and_then(|x|x.to_str()).unwrap_or("").to_ascii_lowercase();
            let kind=if ["png","jpg","jpeg","gif","webp","bmp","svg","mp4","mov","mkv","webm","avi"].contains(&ext.as_str()){"media"}else{"document"};
            let hint=if kind=="media"{"image"}else{"document"};
            eprintln!("[Fatir WhatsApp] FILE_VALIDATED {}",canonical.display());
            let baseline=whatsapp_message_snapshot(&filename).await?;
            let upload=browser_upload_file(&canonical.display().to_string(),Some(hint)).await?;
            eprintln!("[Fatir WhatsApp] FILE_INJECTED {} {}",filename,upload.lines().next().unwrap_or(""));
            let preview=whatsapp_wait_attachment_preview(&filename,kind).await?;
            eprintln!("[Fatir WhatsApp] PREVIEW_CONFIRMED {} {}",filename,preview);
            whatsapp_click_send(&filename,kind).await?;
            eprintln!("[Fatir WhatsApp] SEND_CLICKED {}",filename);
            let verified=whatsapp_wait_sent(&filename,kind,&baseline).await?;
            eprintln!("[Fatir WhatsApp] MESSAGE_VERIFIED {} {}",filename,verified);
            sent.push(canonical.display().to_string());
        }
        Ok(json!({"destination":"whatsapp","recipient":opened,"sent":sent.len(),"paths":sent,"verified":true,"virtual_pointer":true,"physical_mouse_moved":false}))
    }.await;
    hide_all_virtual_pointers().await;
    result
}

pub(crate) async fn share_email_draft_files(paths: &[PathBuf], to: &str, subject: &str) -> Result<Value> {
    let result: Result<Value> = async {
        pointer::ensure_enabled()?;
        let mut url=url::Url::parse("https://mail.google.com/mail/")?;
        url.query_pairs_mut().append_pair("view","cm").append_pair("fs","1").append_pair("to",to);
        if !subject.is_empty(){url.query_pairs_mut().append_pair("su",subject);}
        browser_open_url(url.as_str()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        let mut attached=Vec::new();
        for path in paths {
            browser_upload_file(&path.display().to_string(),Some("attachment")).await?;
            attached.push(path.display().to_string());
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
        Ok(json!({"destination":"email","to":to,"subject":subject,"attached":attached.len(),"paths":attached,"status":"draft_open","note":"Fatir created the Gmail compose draft and attached the files but did not press Send."}))
    }.await;
    hide_all_virtual_pointers().await;
    result
}
