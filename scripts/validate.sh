#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

bash -n "$ROOT/install.sh"
bash -n "$ROOT/uninstall.sh"
bash -n "$ROOT/scripts/setup-desktop-access.sh"

if command -v node >/dev/null 2>&1; then
  node --check "$ROOT/ui/app.js"
fi

python3 - "$ROOT" <<'PY'
from pathlib import Path
import json, re, sys, tomllib
root=Path(sys.argv[1])
tauri_conf=json.loads((root/'src-tauri/tauri.conf.json').read_text())
json.loads((root/'src-tauri/capabilities/default.json').read_text())
json.loads((root/'cinnamon-applet/fatir@local/metadata.json').read_text())
cargo=tomllib.loads((root/'src-tauri/Cargo.toml').read_text())
assert cargo['package']['version']=='1.2.0'
assert tauri_conf.get('version')=='1.2.0', 'Tauri bundle version mismatch'
assert (root/'src-tauri/src/adaptive.rs').exists()
assert (root/'src-tauri/src/browser_memory.rs').exists()
assert (root/'src-tauri/src/teach.rs').exists()
for module in ('orchestrator.rs','recovery.rs','projects.rs','terminal_sessions.rs','permissions.rs','app_playbooks.rs','schedules.rs'):
    assert (root/'src-tauri/src'/module).exists(), f'Missing V1 module {module}'
assert (root/'src-tauri/src/pointer.rs').exists()
assert (root/'src-tauri/src/share.rs').exists()
assert (root/'nemo-actions/fatir-share.nemo_action').exists()
assert (root/'ollama/Fatir-Modelfile').exists()
assert (root/'V1-RELEASE.md').exists(), 'Missing V1 release gate/smoke-test document'

ollama=(root/'src-tauri/src/ollama.rs').read_text()
adaptive=(root/'src-tauri/src/adaptive.rs').read_text()
browser_memory=(root/'src-tauri/src/browser_memory.rs').read_text()
teach=(root/'src-tauri/src/teach.rs').read_text()
assert 'pub fn explicit_headless' in adaptive and 't=="headless"' in adaptive, 'Headless must require explicit literal headless intent'
for case in (
    'Click the new tab in Nemo and rename the local file',
    'Run curl https://example.com/api in the terminal',
    'Run this in the background',
):
    assert case in adaptive, f'Missing local/headless routing regression case: {case}'
assert 'let strong_local = local_path || local_cli.iter().any' in adaptive, 'Strong local context must suppress accidental browser routing'
for case in ('Open GIMP','Launch VLC','Control Calculator','Can you operate installed apps?','Can you control installed applications?','Check your local desktop capabilities.'):
    assert case in adaptive, f'Missing installed-app desktop routing regression case: {case}'
assert 'relation_score' in browser_memory and '.filter_map(|x|{let relation=relation_score' in browser_memory, 'Browser memory must reject unrelated selector candidates'
assert 'semantic_for_uid_on_url' in browser_memory, 'Stale UID recovery must be scoped to the current site'
assert 'Teach Mode records stable actions, not debugging noise' in teach, 'Teach Mode replay-safety guard missing'
assert '"browser_click"|"browser_tabs"|"browser_select_tab"|"browser_close_tab"' in teach, 'Teach Mode must not record coordinate clicks/transient tab IDs'
assert 'const BROWSER_CONTROL_MODEL: &str = "gemma4:31b-cloud";' in ollama, 'Browser control model must remain Gemma 4 31B Cloud'
assert 'if browser_control || desktop_control || has_images { return BROWSER_CONTROL_MODEL.into(); }' in ollama, 'Auto browser/interactive-desktop ownership regressed'
assert '"cloud" | "cloud-only" => if browser_control || desktop_control || has_images { BROWSER_CONTROL_MODEL.into() }' in ollama, 'Cloud browser/desktop ownership regressed'
assert 'model = cloud_owner_for_hint(&hint, has_images);' in ollama, 'Cloud fallback must preserve browser ownership'
assert 'Blocked browser execution because this turn is classified as local/non-browser work' in ollama, 'Agent loop must hard-block browser tools on local turns'
assert 'Blocked an indirect browser launch because this turn is local/desktop work' in ollama, 'Local turns must block browser launches hidden behind desktop/shell tools'
assert 'name == "desktop_launch_app"' in ollama and '"terminal_session_exec"|"background_job_start"|"scheduled_job_create"' in ollama, 'Indirect browser launch guard must cover desktop launcher and every shell-bearing local execution surface' 
assert 'Blocked headless browser execution because the current user request did not explicitly request headless mode' in ollama, 'Agent loop must hard-block implicit headless use'
assert 'if name == "browser_takeover"' in ollama and 'Browser control is paused for your manual step' in ollama, 'Human takeover must end the active agent run'
assert 'Never switch to headless automatically' in ollama, 'System prompt must prohibit implicit headless fallback'
for keyword in ('whatsapp','gmail','amazon','search online'):
    assert keyword in adaptive.lower(), f'Browser intent coverage missing {keyword}'
# Browser tasks must not be forced through the one-shot Vision Bridge back to the text model.
assert 'Browser tasks are different: in Auto/Cloud modes Gemma 4 31B owns the complete browser tool loop directly' in ollama
assert 'Interactive desktop GUI workflows are handled the same way' in ollama, 'Interactive desktop workflows must stay multimodal-owned'
assert 'fn is_interactive_desktop_hint' in ollama, 'Interactive desktop model routing missing'
assert 'Explicit local-first policy outranks historical learned routing' in ollama, 'Local/desktop work must outrank stale cloud-route learning'
assert 'if let Some(m) = preferred_deep_local.clone() { return m; }' in ollama, 'Local-first must fall back to an available local deep model when fatir-router is absent'
assert 'desktop capabilities' in ollama and 'capabilit' in ollama, 'Exact desktop capability checks must use deterministic local fast path'
fast_start=ollama.index('let asks_desktop_capability =')
fast_end=ollama.index('// Basic PC status is deterministic', fast_start)
fast_block=ollama[fast_start:fast_end]
assert 'Whole-desktop application control is available locally.' in fast_block, 'Generic desktop capability fast path must report whole-desktop readiness'
assert 'desktop_apps' in fast_block and 'desktop_windows' in fast_block, 'Generic desktop capability fast path must inspect installed apps and running windows'
assert 'Android Studio' not in fast_block, 'Generic desktop capability fast path must not contain app-specific Android Studio wording'
assert 'whole-desktop Linux capability check' in fast_block, 'Generic desktop trace must remain app-agnostic'
assert 'Internal tool-call syntax is NEVER user-facing' in ollama, 'Fatir must not tell users to paste internal tool JSON'
assert 'python3-pyatspi' in ollama and 'never `python3-atspi`' in ollama, 'Desktop prompt must name the correct Mint/Ubuntu AT-SPI Python package'

orchestrator=(root/'src-tauri/src/orchestrator.rs').read_text()
recovery=(root/'src-tauri/src/recovery.rs').read_text()
projects=(root/'src-tauri/src/projects.rs').read_text()
terminal=(root/'src-tauri/src/terminal_sessions.rs').read_text()
permissions=(root/'src-tauri/src/permissions.rs').read_text()
assert 'pub fn requires_post_verification' in orchestrator and 'pub fn is_verifier' in orchestrator, 'V1 verification gate helpers missing'
assert 'FATIR V1 VERIFICATION GATE' in orchestrator and 'verification_debt' in ollama, 'Agent-loop verification gate missing'
assert 'FATIR V1 RECOVERY' in recovery and 'failed_calls' in ollama, 'V1 recovery/loop guard missing'
assert 'KNOWN LOCAL PROJECTS' in projects and 'git_branch' in projects, 'Project continuity missing'
assert 'PERSISTENT TERMINAL SESSIONS' in terminal and 'TerminalSession' in terminal, 'Persistent terminal continuity missing'
assert 'always_confirm' in permissions and 'destructive actions' in permissions, 'Permission policy invariant missing'
assert 'Teach Mode 3.0' in teach and 'verify_with' in teach, 'Teach Mode 3.0 verification metadata missing'
playbooks=(root/'src-tauri/src/app_playbooks.rs').read_text()
for family in ('android studio','nemo','gimp','krita','libreoffice','vlc','calculator','system settings','archive manager'):
    assert family in playbooks.lower(), f'V1 app playbook missing {family}'
assert 'LOCAL ACTION MEMORY' in (root/'src-tauri/src/memory.rs').read_text(), 'Cross-surface action memory missing'
assert 'V1 multi-step task completion requires a verification checkpoint with evidence' in (root/'src-tauri/src/tasks.rs').read_text() and 'checkpoints' in (root/'src-tauri/src/tasks.rs').read_text(), 'V1 task checkpoint engine missing'
ui=(root/'ui/index.html').read_text(); appjs=(root/'ui/app.js').read_text()
assert 'Fatir V1' in ui and 'opsProjects' in ui and 'opsRuntime' in ui, 'V1 Command Center UI missing'
assert 'confirmFileChanges' in ui and 'refreshPermissions' in appjs, 'V1 permission controls missing'
assert (root/'ui/assets/fatir-mark.png').is_file(), 'Arabic calligraphy sidebar mark missing'
assert (root/'cinnamon-applet/fatir@local/icon.png').is_file(), 'Cinnamon calligraphy icon missing'
assert 'assets/fatir-mark.png' in ui and 'closePanelBtn' in ui, 'V1.1 sidebar brand/hide controls missing'
assert 'panelSide' in ui and 'panelWidth' in ui, 'V1.1 sidebar layout settings missing'
assert "invoke('set_panel_layout'" in appjs and "invoke('hide_panel')" in appjs, 'V1.1 panel layout/hide integration missing'
main_src=(root/'src-tauri/src/main.rs').read_text()
assert 'fn set_panel_layout(' in main_src and 'fn hide_panel(' in main_src and 'apply_panel_layout' in main_src, 'V1.1 native panel layout commands missing'
assert tauri_conf['app']['windows'][0]['width']==420 and tauri_conf['app']['windows'][0]['minWidth']==360, 'V1.1 sidebar window sizing regressed'
styles=(root/'ui/styles.css').read_text()
assert 'v1.1 — Noor sidebar shell' in styles and '--accent:#b98630' in styles, 'V1.1 light/gold visual shell missing'
applet=(root/'cinnamon-applet/fatir@local/applet.js').read_text()
assert "/icon.png" in applet, 'Cinnamon applet must use the selected calligraphy mark'

tasks_src=(root/'src-tauri/src/tasks.rs').read_text()
assert 'verification checkpoint with evidence' in tasks_src, 'V1 multi-step task completion must require verification evidence'
assert 'Automatic post-approval verification failed' in (root/'src-tauri/src/ollama.rs').read_text() and 'verification_evidence' in (root/'src-tauri/src/ollama.rs').read_text(), 'Approved GUI mutations must retain V1 post-action verification'
assert 'Verification checkpoints require concrete evidence' in tasks_src, 'Verification checkpoints must not accept empty evidence'

tools=(root/'src-tauri/src/tools.rs').read_text()
assert 'Browser startup blocked: the current turn does not have explicit browser intent' in tools, 'Browser startup must be blocked at the lowest execution layer without explicit intent'
assert 'with_browser_execution_context' in tools and 'browser_context_for_session(&state, session_id).await' in ollama, 'Browser permission scope must preserve browser surface/session context for the current turn'
assert 'is_active_browser_request' in adaptive and 'current chrome session' in adaptive.lower(), 'Active/current browser-session routing phrases missing'
assert 'tool_message_is_successful_surface_evidence' in ollama and 'tool_is_browser_surface_evidence' in ollama, 'Browser continuity must use successful browser tool evidence'
assert 'Successful browser tool use' in ollama or 'successful browser tool use' in ollama, 'Browser continuity rationale missing'
assert 'blocked browser attempt' in ollama.lower() or 'blocked/failed browser attempts' in ollama.lower(), 'Blocked browser attempts must not establish continuity'
for case in ('successful_browser_tools_keep_terse_followups_on_browser_surface','active_chrome_mode_survives_terse_followups_after_browser_tools','explicit_local_switch_breaks_browser_continuity','blocked_browser_attempt_does_not_create_surface_continuity'):
    assert case in ollama, f'Missing browser-surface continuity regression test: {case}'
assert 'ACTIVE_MCP_SESSION' in tools and '--auto-connect' in tools, 'Active browser session must attach through a dedicated MCP auto-connect session'
assert 'Fatir will not launch its own browser for an active-session request' in tools, 'Active browser requests must never launch the managed browser'
assert 'Direct CDP fallback is disabled for active-browser-session mode' in tools, 'Active browser mode must not fall through to managed-browser CDP'
assert 'Previous browser context:' in ollama and 'Current follow-up:' in ollama, 'Browser follow-up continuity missing'
assert 'Short follow-ups such as' in ollama and 'inherit that active-browser context' in ollama, 'System prompt must preserve active-browser follow-up semantics'
assert 'active_browser_phrases_mean_existing_user_chrome' in adaptive, 'Active-browser routing regression tests missing' 
for required in (
    'browser_tabs','browser_new_tab','browser_select_tab','browser_close_tab','browser_extract_structure',
    'browser_network_recent','browser_network_request','browser_console_messages','browser_diagnostics','browser_memory_current',
    'browser_takeover','browser_takeover_resume','browser_takeover_status',
    'headless_browser_tabs','headless_browser_open_url','headless_browser_extract_structure','headless_browser_network_recent',
    'headless_browser_network_request','headless_browser_console_messages','headless_browser_observe','headless_browser_stop',
    'teach_start','teach_status','teach_stop','teach_cancel',
    'task_set_phase','task_checkpoint','task_record_error','task_resume','project_inspect','project_remember','project_list','project_forget',
    'terminal_session_create','terminal_session_list','terminal_session_set_cwd','terminal_session_exec','terminal_session_history','terminal_session_close','fatir_v1_status','desktop_playbook','scheduled_job_create','scheduled_job_list','scheduled_job_cancel',
    'desktop_doctor','desktop_repair_accessibility','desktop_apps','desktop_windows','desktop_capabilities','desktop_key','desktop_type','desktop_observe','desktop_visual_action'
):
    assert f'"{required}"' in tools, f'Missing 0.8 browser/teach tool {required}'
assert 'if browser_request && !explicit_headless' in tools, 'Visible browser schemas must be gated by browser intent'
assert 'if explicit_headless {' in tools and 'headless_browser_stop' in tools, 'Headless schemas must be explicit-only'
assert 'Blocked shell-level headless execution because the current user request did not explicitly request headless mode' in (root/'src-tauri/src/ollama.rs').read_text(), 'Explicit-only headless policy must cover shell/background/schedule execution too'
assert 'scheduled_job_create' in tools and 'scheduled_job_list' in tools and 'scheduled_job_cancel' in tools, 'V1 scheduled local job tools missing'
assert 'remind me' in tools and 'every day' in tools, 'Schedule intent must expose scheduled local job tools'
schedules=(root/'src-tauri/src/schedules.rs').read_text()
assert 'Persistent=true' in schedules and '.config/systemd/user' in schedules, 'V1 schedules must use persistent systemd user unit files rather than transient timers'
agent_schedules=(root/'src-tauri/src/agent_schedules.rs').read_text()
assert 'agent_schedule_create' in tools and 'agent_schedule_list' in tools and 'agent_schedule_cancel' in tools, 'Agent/web schedule tools missing'
assert 'Persistent=true' in agent_schedules and 'browser_mode' in agent_schedules and 'credential_ids' in agent_schedules, 'Agent schedules must persist explicit browser and credential grants'
assert 'with_execution_grant' in (root/'src-tauri/src/ollama.rs').read_text(), 'Scheduled credential execution grant missing'
assert (root/'src-tauri/src/chat_history.rs').exists(), 'Shared chat history module missing'
companion=(root/'src-tauri/src/companion.rs').read_text()
assert '/api/v1/chats' in companion and '/api/v1/agent/schedules' in companion, 'Companion chat/schedule sync API missing'
assert 'browser_fill_credential_by_label' in tools, 'Semantic credential broker browser fill missing'
assert 'browser_takeover' in tools, 'Human authentication takeover support missing'
assert 'enable\",\"--now' in schedules and 'daemon-reload' in schedules, 'Persistent schedules must reload and enable their user timer'
assert 'persistent_across_reboot' in schedules, 'Schedule result must report persistent timer semantics'
assert 'timer was rolled back' in schedules and 'disable","--now' in schedules, 'Schedule registry-save failures must roll back the enabled timer'
assert 'items.sort_by(|a,b|b.created_at.cmp(&a.created_at));' in schedules and 'save(&items)' in schedules and 'items.truncate(100)' not in schedules, 'Schedule records must not be truncated while timers may still be enabled'
jobs=(root/'src-tauri/src/jobs.rs').read_text()
assert 'fatir-job-{short}.service' in jobs and 'ah-job-{short}.service' not in jobs, 'V1 background units must use Fatir naming'
assert 'cwd.map(expand_path)' in jobs, 'Background jobs must expand ~/ working directories'
assert 'jobs.truncate(' not in jobs, 'Background-job records must not be truncated while detached jobs may still be controllable'
assert '"terminal_session_exec"|"background_job_start"|"scheduled_job_create"' in (root/'src-tauri/src/ollama.rs').read_text(), 'Local surface lock must cover shell-bearing terminal/background/schedule tools'
assert '!explicit_headless && h.contains("amazon")' in tools, 'Amazon fast path must not hijack explicit headless runs'
assert 'takeover_path().is_file()' in tools, 'Takeover resume/status must remain available on the next user turn'
assert 'teach::is_active()' in tools, 'Teach controls must remain available while recording'
assert 'let preferred_url = if CHROME_MCP_READY.load(Ordering::SeqCst)' in tools and 'chrome_mcp_current_url().await.ok()' in tools, 'CDP fallback/upload must follow MCP-selected tab when MCP is healthy'
assert 'redact_sensitive_browser_text' in tools and 'sensitive_headers_redacted' in tools, 'Network diagnostics must redact secrets'
assert 'human_takeover_recommended' in tools, 'Browser snapshots must surface human-challenge detection'
assert 'const HEADLESS_MCP_SESSION: &str = "fatir-headless";' in tools, 'Headless MCP must use isolated daemon session'
# A normal visible snapshot should perform one MCP snapshot call, not duplicate it.
snap_start=tools.index('async fn chrome_mcp_snapshot()')
snap_end=tools.index('fn semantic_text_score',snap_start)
assert tools[snap_start:snap_end].count('chrome_mcp_call("take_snapshot"')==1, 'Visible MCP snapshot helper must snapshot exactly once'
assert 'const CHROME_DEVTOOLS_MCP_VERSION: &str = "1.10.1";' in tools, 'Pinned Chrome DevTools MCP version missing'
assert 'chrome_mcp_call("take_snapshot"' in tools, 'MCP accessibility snapshot integration missing'
assert 'chrome_mcp_call("click"' in tools, 'MCP click integration missing'
assert 'chrome_mcp_call("fill"' in tools, 'MCP fill integration missing'
assert 'chrome_mcp_call("navigate_page"' in tools, 'MCP navigation integration missing'
assert 'if let Ok(result)=chrome_mcp_elements().await { return Ok(result); }' in tools, 'browser_elements must prefer MCP'
assert 'if mcp_uid.is_match(element_id) { return chrome_mcp_click_uid(element_id).await; }' in tools, 'MCP uid click path missing'
ui=(root/'ui/app.js').read_text()
assert "invoke('project_list')" in ui and "invoke('terminal_session_list')" in ui and "invoke('v1_status')" in ui, 'V1 Command Center must fetch project/terminal/runtime state before rendering it'
assert 'agent schedule(s)' in ui and 'agentScheduleList' in ui, 'V1.2 Command Center agent schedule status missing'

install=(root/'install.sh').read_text()
assert 'NODE_MAJOR == 22 && NODE_MINOR >= 12' in install, 'Installer must enforce Chrome DevTools MCP Node 22.12+ compatibility'
assert 'Building Fatir v1.2.0' in install, 'Installer version label mismatch'
assert './scripts/validate.sh' in install and 'Validating Fatir v1.2.0 source' in install, 'Installer must run source validation before Cargo build'
assert '[9/9] Starting Fatir' in install and '[1/8]' not in install and '[8/8]' not in install, 'V1 installer progress numbering must be internally consistent'
assert 'chrome-devtools-mcp@${MCP_VERSION}' in install, 'Installer must provision Chrome DevTools MCP'
assert 'setup_22.x' in install, 'Installer must ensure compatible Node.js for MCP'
for pkg in ('python3-pyatspi','python3-cairo','gir1.2-atspi-2.0','gir1.2-gtk-3.0','libatk-wrapper-java','libatk-wrapper-java-jni','xdotool','wmctrl','xclip','x11-utils','xauth','gnome-screenshot','scrot','xdg-utils','desktop-file-utils','libglib2.0-bin','libgtk-3-bin','dbus-x11','libnotify-bin','policykit-1','gvfs-backends','shared-mime-info','gnome-keyring','flatpak','poppler-utils','unzip','tar','procps','psmisc','git'):
    assert pkg in install, f'Installer missing local runtime dependency {pkg}'
assert '$SCRIPT_DIR/scripts/setup-desktop-access.sh' not in install, 'Installer must not reference undefined SCRIPT_DIR'
assert '$ROOT/scripts/setup-desktop-access.sh' in install, 'Installer must run desktop setup from ROOT'
assert 'python3-atspi' not in install, 'Wrong package name python3-atspi must never be installed'
security=(root/'SECURITY.md').read_text()
assert 'Chrome DevTools MCP' in security and 'Headless browser tools are opt-in only' in security, 'Security model must document MCP and explicit-only headless'
assert 'V1 execution and verification' in security and 'Local schedules' in security, 'Security model must document V1 verification and schedule boundaries'
permissions_src=(root/'src-tauri/src/permissions.rs').read_text()
assert 'confirm_shell_commands:false' in permissions_src, 'Routine user-level shell commands should be low-friction by default in V1'
assert 'shell_command_risk' in tools and 'run_shell_with_credentials' in tools and 'agent_schedule_create' in tools and '=> "sensitive"' in tools, 'Shell approvals must be risk-aware and credential/agent-schedule actions always sensitive'
assert 'sudo or pkexec' in tools, 'Ordinary shell execution must not bypass the privileged PolicyKit tool'

setup=(root/'scripts/setup-desktop-access.sh').read_text()
for pkg in ('python3-pyatspi','python3-cairo','gir1.2-atspi-2.0','gir1.2-gtk-3.0','libatk-wrapper-java','libatk-wrapper-java-jni','xdotool','wmctrl','xclip','x11-utils','xauth','gnome-screenshot','scrot','xdg-utils','desktop-file-utils','libglib2.0-bin','libgtk-3-bin','dbus-x11','libnotify-bin','policykit-1','gvfs-backends','shared-mime-info','gnome-keyring','flatpak','poppler-utils','unzip','tar','procps','psmisc','git'):
    assert pkg in setup, f'Manual local runtime setup missing {pkg}'
for cmd in ('xclip','xprop','gio','xdg-open','gsettings','gdbus','gtk-launch','update-desktop-database','notify-send','pkexec','dpkg-query','dpkg-deb','pdfinfo','pdftotext','pdftoppm','systemd-run','sha256sum','git'):
    assert cmd in setup, f'Local runtime setup must verify command {cmd}'
assert 'python3-atspi' not in setup, 'Manual setup must not use wrong python3-atspi package'
assert 'ide.support.screenreaders.enabled=true' in setup and 'javax.accessibility.assistive_technologies=org.GNOME.Accessibility.AtkWrapper' in setup, 'JetBrains accessibility setup missing'
for product in ('AndroidStudio','intellijidea','pycharm','webstorm','clion','phpstorm','goland','datagrip','rubymine','rider','rustrover'):
    assert product.lower() in setup.lower(), f'JetBrains setup missing {product}'
assert 'FATIR_SKIP_APT=1' in install and 'setup-desktop-access.sh' in install, 'Installer must enable the generic desktop-control stack after dependencies'
wa_start=tools.index('async fn whatsapp_message_snapshot')
wa_end=tools.index('pub(crate) async fn share_email_draft_files', wa_start)
wa=tools[wa_start:wa_end]
assert "accept==='*'" in tools, 'Document wildcard input handling regressed'
assert "if(kind==='document'){{" in tools, 'Rust format-string braces around document scoring regressed'
assert "        }}\n        if(el.multiple)s+=3;" in tools, 'Rust format-string closing brace around document scoring regressed'
assert 'data-fatir-whatsapp-send' in wa, 'Verified WhatsApp Send target marker missing'
for unsafe in ('forward','share','reply','delete','cancel','remove','next'):
    assert unsafe in wa.lower(), f'WhatsApp unsafe-target blacklist missing {unsafe}'
assert 'browser_key("Enter")' not in wa, 'WhatsApp attachment Send must not fall back to Enter'
assert 'MESSAGE_VERIFIED' in wa, 'WhatsApp post-send verification logging missing'
assert 'new_outgoing' in wa, 'WhatsApp success must require a new outgoing message'
assert 'active && input_has_file && media' in wa, 'WhatsApp media preview must accept verified visual media without requiring filename text'
assert 'active && input_has_file && filename_visible' in wa, 'WhatsApp document preview must still require filename verification'
assert 'whatsapp_click_send(&filename,kind)' in wa, 'WhatsApp Send verification lost attachment kind'
assert 'whatsapp_wait_sent(&filename,kind,&baseline)' in wa, 'WhatsApp post-send verification lost attachment kind'
assert 'whatsapp_attachment_state(filename,kind)' in wa, 'WhatsApp post-send state must preserve attachment kind'
# Rust format strings that embed JavaScript must escape literal braces as {{ and }}.
# This catches the class of compile failure fixed in 0.7.4 before packaging.
fmt_blocks=re.findall(r'format!\(r#"(.*?)"#(?:,.*?)?\)', tools, re.S)
placeholder=re.compile(r'\{[A-Za-z_][A-Za-z0-9_]*(?::[^}]*)?\}')
for n, block in enumerate(fmt_blocks, 1):
    i=0
    while i < len(block):
        if block.startswith('{{', i) or block.startswith('}}', i):
            i += 2
            continue
        if block[i] == '{':
            m=placeholder.match(block, i)
            assert m, f'Rust format block {n} has an unescaped opening brace near: {block[i:i+60]!r}'
            i=m.end()
            continue
        assert block[i] != '}', f'Rust format block {n} has an unescaped closing brace near: {block[max(0,i-30):i+30]!r}'
        i += 1

# Every external Rust command must be represented by the local-runtime doctor,
# except shell interpreters and explicitly optional fallback programs.
all_rust='\n'.join(x.read_text() for x in (root/'src-tauri/src').glob('*.rs'))
rust_commands=set(re.findall(r'(?:std::process::)?Command::new\(\"([^\"]+)\"\)', all_rust))
desktop=(root/'src-tauri/src/desktop.rs').read_text()
declared_commands=set(re.findall(r'\(\"([^\"]+)\", \"[^\"]+\"\)', desktop[desktop.index('const LOCAL_RUNTIME_COMMANDS'):desktop.index('];', desktop.index('const LOCAL_RUNTIME_COMMANDS'))]))
allowed_implicit={'bash','/bin/bash','sh','flameshot'}
undeclared=sorted(rust_commands-declared_commands-allowed_implicit)
assert not undeclared, f'Rust external commands missing from LOCAL_RUNTIME_COMMANDS: {undeclared}'
for needle in ('pub async fn doctor()', 'pub async fn repair_accessibility()', 'pub async fn apps(', 'pub async fn windows()', 'pub async fn capabilities(', 'pub async fn visual_action(', 'python3-pyatspi', 'libatk-wrapper-java-jni', 'desktop-file-utils', 'dbus-x11', 'gnome-keyring', 'LOCAL_RUNTIME_COMMANDS', 'missing_runtime_commands', 'secret_service_ready', 'credential_keyring_ready', 'ide.support.screenreaders.enabled=true', '-Djavax.accessibility.assistive_technologies=org.GNOME.Accessibility.AtkWrapper', 'pub async fn key(', 'pub async fn type_text('):
    assert needle in desktop, f'Desktop 4.0 regression: missing {needle}'
assert '/var/lib/flatpak/exports/share/applications' in desktop and '/var/lib/snapd/desktop/applications' in desktop, 'Installed-app discovery must include Flatpak and Snap launchers'
assert 'Merged AT-SPI and X11 windows' in desktop, 'Window discovery must merge semantic and X11 surfaces'
assert 'physical_pointer_restored' in desktop, 'Visual X11 fallback must report pointer restoration'
assert 'QT_ACCESSIBILITY' in desktop and 'ACCESSIBILITY_ENABLED' in desktop, 'Fatir-launched apps must receive accessibility environment hints'
assert 'Command::new("scrot").args(["-u"' in desktop, 'Desktop observe must have scrot screenshot fallback'
assert 'command_available("gnome-screenshot").await || command_available("scrot").await' in desktop, 'Desktop readiness must accept either screenshot backend'
assert 'Command::new("scrot").arg(file.to_string_lossy().as_ref())' in tools, 'General screenshot tool must have scrot fallback'
assert 'desktop_activate_named" | "desktop_visual_action" | "desktop_key' in tools, 'Desktop destructive-intent risk gate missing'
assert 'apt-get install -y python3-atspi' not in desktop and '"python3-atspi",' not in desktop, 'Desktop code must not install the nonexistent python3-atspi package'
m=re.search(r'const PY: &str = r#"\n(.*?)\n"#;', desktop, re.S)
if not m:
    raise SystemExit('Could not locate embedded desktop helper')
compile(m.group(1), 'desktop_helper.py', 'exec')
pointer=(root/'src-tauri/src/pointer.rs').read_text()
pm=re.search(r'const OVERLAY_PY: &str = r#"\n(.*?)\n"#;', pointer, re.S)
if not pm:
    raise SystemExit('Could not locate embedded virtual pointer helper')
compile(pm.group(1), 'fatir_pointer_overlay.py', 'exec')

# v1.1.1 Command Center must be a full-height sidebar workspace, not the legacy bottom sheet.
assert '#opsSheet{align-items:stretch' in styles, 'Command Center must align from the top/full height'
assert '#opsSheet .command-center{height:100%;max-height:none' in styles, 'Command Center card must fill the sheet height'
assert '#opsSheet .ops-panel{flex:1 1 auto;min-height:0;overflow-y:auto' in styles, 'Command Center content must scroll inside the panel'
assert '.shell{height:calc(100vh - 16px);min-height:0' in styles or '.shell{height:calc(100vh - 12px)' in styles, 'Sidebar shell must use viewport-relative height'
print('Static validation passed')
PY
