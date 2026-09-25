# Changelog

## 1.2.0 — Scheduled Agents, Shared Chats & Credential Login

- Added persistent model/web schedules backed by systemd user timers without conflating them with local shell schedules.
- Added explicit schedule execution modes for normal agent, managed browser, active browser and opt-in isolated headless browser work.
- Added per-schedule stored-credential authorization: only selected credential IDs can be reused automatically and secret values remain in Linux Secret Service.
- Added semantic browser credential filling by label/placeholder so stored passwords can be used without exposing them to the model.
- Added Google sign-in guidance to reuse an authenticated browser session when appropriate.
- Added mandatory human takeover for authenticator/TOTP, security keys, CAPTCHA, phone approval and unusual verification.
- Added persistent shared chat indexing and history across Fatir Desktop, Companion and scheduled-agent sessions.
- Added Companion chat-history and scheduled-agent API/screens over the existing authenticated LAN/Tailscale connection.
- Moved the Companion connection token into Android Keystore-backed AES-GCM storage with migration from the previous preference value.
- Scheduled runs persist last status/result/model and appear in the same local chat record; publishing workflows remain verification-gated.


## 1.1.1 — Full-height Command Center

- Fixed Command Center rendering as a short bottom sheet on tall sidebar windows.
- Command Center now fills the available sidebar height from the top, with the active section scrolling internally.
- Preserved the compact summary/tabs while allowing task, job, routine, project and runtime content to use the full panel.
- Replaced the legacy fixed 744px shell fallback with viewport-relative height so the sidebar scales correctly across monitor resolutions.
- Added regression validation for the full-height Command Center layout.

## 1.1.0 — Light Sidebar Experience
- Reworked the resident Tauri window into a refined light sidebar without changing the agent/runtime architecture.
- Added the selected Arabic calligraphy brand mark to the sidebar, bundle icons and Cinnamon applet.
- Added adjustable 360–520 px panel width and left/right monitor-edge placement.
- Added an explicit hide button while preserving pin/autohide behavior.
- Split the narrow header into a calm brand row and compact action/model row so controls stay usable at sidebar widths.
- Restyled chat, traces, task sheets, approvals, settings, share UI and cards with warm cream/gold/green visual language.

## 1.0.4 — Persistent Browser/Local Surface Continuity

- Fixed browser tasks losing their surface after several terse follow-ups.
- Successful `browser_*` actions now persist browser context until the user explicitly switches to Linux/files/terminal/desktop work.
- Failed or blocked browser attempts are ignored as continuity evidence.
- Active/current-Chrome mode survives terse follow-ups without opening Fatir's managed Chrome.
- Browser-context follow-ups route to Gemma 4 in Auto/Cloud instead of Local Router/GPT-OSS 20B.

## 1.0.3 — Active Browser Session + Browser Context Continuity

- `active/current browser session` and `current Chrome session` now mean the exact Chrome instance the user already has open.
- Active-session mode attaches through Chrome DevTools MCP `--auto-connect`; it never launches Fatir's managed browser as a substitute.
- Short browser follow-ups such as `it's the 3rd tab` inherit the prior browser surface instead of being reclassified as local/general.
- Explicit local/desktop requests break browser continuity immediately.
- Browser routing now recognizes current/active Chrome, browser session, and Chrome/browser-tab wording.
- Browser tasks continue to be owned by Gemma 4 31B in Auto/Cloud modes.
- Direct CDP fallback is disabled in active-session mode so it cannot accidentally target Fatir's separate browser profile.
- If active Chrome cannot be attached, Fatir stops with a one-time remote-debugging setup instruction instead of opening another Chrome.


## 1.0.2 — Generic Whole-Desktop Capability Report

- Removed Android Studio-specific wording from the generic desktop capability fast path.
- Whole-desktop readiness now reports installed graphical application discovery, running-window discovery, semantic accessibility coverage, keyboard/X11 fallback and visual fallback.
- Generic installed-app capability checks remain deterministic (`Fast Path · 0 LLM`) and never start Chrome.
- Added a regression guard so the generic capability path cannot mention Android Studio unless a future app-specific path explicitly asks for it.

## 1.0.1 — Local Runtime Routing Hotfix

- Desktop capability checks (including “Check your local desktop capabilities”) now bypass all models and run `desktop_doctor` directly.
- Auto local-first routing now falls back from `fatir-router` to an available local 4B model instead of silently dropping to GPT-OSS 20B.
- Browser startup now has a task-local execution permission gate inside `ensure_fatir_browser`; local/non-browser turns cannot spawn the Fatir Chrome session even if a higher-level routing bug occurs.
- Approval/resume paths preserve browser permission only for browser-originated actions.

## 1.0.0 — Unified Personal Linux Operator

- Added the V1 execution lifecycle: **UNDERSTAND → PLAN → ACT → VERIFY → RECOVER/COMPLETE** for substantial tasks.
- Added an enforced post-action verification gate for browser and desktop GUI mutations; Fatir cannot finalize success after an unverified click/type/action.
- Extended persistent tasks with phases, evidence checkpoints, error state, attempts, and resume counters. Multi-step tasks cannot be marked complete without a verification checkpoint containing concrete evidence.
- Added recovery classification and identical-failure loop guards for permissions, missing dependencies, timeouts, network/authentication failures, stale browser targets, desktop accessibility state and build failures.
- Added persistent project registry and project inspection: project type, Git branch/status, manifests and recent commits.
- Added persistent logical terminal sessions with remembered cwd and bounded command/output history; long-running detached work still uses background jobs.
- Added persistent local schedules through systemd user timers for explicitly requested reminders/recurring local commands; scheduling never grants browser/headless permission.
- Added cross-surface successful-action memory as advisory context while preserving approval and verification boundaries.
- Upgraded Teach Mode to **3.0**: replay-safe semantic steps now include control surface and verification hints. Secrets, coordinates, stale browser UIDs and diagnostic noise remain excluded.
- Added application playbooks for common Linux app families while preserving generic AT-SPI → keyboard → visual fallback control for everything else.
- Added a user-configurable approval policy for ordinary reversible file/shell/background/routine-metadata work. Destructive actions, PolicyKit/system changes and stored credentials always require approval.
- Added the V1 Command Center with project, terminal, task-phase, Teach and recovery visibility.
- Preserved Chrome DevTools MCP browser intelligence, browser memory, multi-tab/network/console tooling, explicit-only headless policy, whole-desktop control, WhatsApp verified attachment sending and browser/local isolation.
- Added Git to the local-runtime dependency/doctor matrix for project inspection.
- Added V1 regression guards and clean-package validation.

- **Risk-aware shell approval:** harmless user-level commands can run without redundant prompts by default, while delete/uninstall/wipe, privileged/system and credential-bearing commands remain approval-gated.

## 0.8.4 — Local Runtime Hardening

- Audited every external executable used by Fatir local/app operations and expanded the installer/runtime doctor to cover the complete dependency chain.
- Added desktop-file-utils, D-Bus X11 support, GVFS backends, shared MIME data, GNOME Keyring, X auth, scrot fallback, process utilities and the existing clipboard/launcher/notification/PolicyKit/PDF/archive layers to one verified runtime.
- Added command-level desktop doctor checks for window control, clipboard, screenshots, app launching, desktop database refresh, notifications, PolicyKit, Flatpak, PDF tools, archives, systemd jobs and core disk/process utilities.
- Added an actual Secret Service D-Bus probe so credential-keyring readiness is not inferred from package installation alone.
- Added `scrot` as a backup screenshot path for both general and window-scoped capture.
- Fixed an undefined `$SCRIPT_DIR` reference in `install.sh` that could terminate installation under `set -u`; setup now uses `$ROOT/scripts/setup-desktop-access.sh`.
- Keeps Snap optional on Linux Mint: existing Snap launchers are discoverable, but the installer does not enable Snap.

## 0.8.3 — Local Surface Lock

- Classifies “installed apps/applications” questions as desktop intent.
- Explicit “open/launch/start/use Chrome” remains a browser request.
- Adds an execution-level guard that blocks Chrome/Chromium/Firefox/Brave/Edge/Vivaldi launches from `desktop_launch_app` or shell URL openers during local/non-browser turns.
- Keeps browser tools unavailable for local Linux/application questions.

## 0.8.2 — Whole-Desktop Control 4.0

- Generalized local GUI control from Android Studio-specific setup to installed Linux applications across system, user, Flatpak and Snap launchers.
- Merged AT-SPI and X11 window discovery so inaccessible/custom-rendered apps are still discoverable and focusable.
- Added installed-app inventory and per-window capability probing.
- Added layered control: semantic AT-SPI first, keyboard/window fallback second, screenshot vision third, X11 screenshot-relative click/drag/scroll fallback last.
- X11 visual fallback restores the user's original pointer position after each action.
- Added generic JetBrains-family accessibility configuration and Qt/accessibility environment hints for Fatir-launched apps.
- Added dynamic approval detection for destructive GUI intentions such as delete/remove/uninstall/erase/format/empty-trash and Shift+Delete.
- Broadened local routing so `open/launch/start/use/control <installed app>` stays on the desktop surface and cannot wake Chrome accidentally.

## 0.8.1 — Desktop Control + Android Studio Accessibility

- Corrected Linux AT-SPI dependency handling: `python3-pyatspi` is the package name on supported Mint/Ubuntu releases.
- Added deterministic desktop accessibility diagnostics and one-shot PolicyKit repair instead of model-guessed package names.
- Added Java ATK bridge dependencies for Java/Swing IDEs and Android Studio accessibility support.
- Added Android Studio screen-reader bridge configuration with backups and restart guidance.
- Added window-scoped keyboard/type fallbacks for accessible desktop applications without moving the physical mouse.
- Local/desktop tasks remain browser-blocked and headless remains explicit-only.
- Fatir no longer tells users to paste internal tool-call JSON; manual commands are emitted as real shell commands only.

## 0.8.0 — Browser Intelligence + Teach Mode 2.0

- Added context-aware surface routing plus a hard execution guard so local Linux/files/terminal/desktop work cannot accidentally start Chrome.
- Added persistent per-domain browser memory and semantic self-healing for stale/renamed controls; unrelated memory candidates are rejected.
- Added MCP network inspection, failed-request diagnostics and console warning/error awareness with secret/header redaction.
- Added MCP multi-tab list/new/select/close tools and aligned direct-CDP fallback/upload actions to the MCP-selected tab.
- Added structured DOM extraction for headings, links, lists, tables, forms and repeated result/product cards.
- Added deterministic human takeover checkpoints: automation ends immediately on handoff and resumes only after the user returns control, followed by a fresh snapshot.
- Added Teach Mode 2.0 recording into reusable routines while excluding credentials, coordinate clicks, transient UIDs/page IDs and diagnostic noise.
- Added a separate persistent headless MCP session that is available only when the current user request explicitly contains `headless`; it is never entered from `background` wording or as a fallback.
- Added isolated headless tabs, semantic clicks/fills, structured extraction, network/console inspection, screenshots and explicit session stop.
- Fixed duplicate MCP snapshot calls and made selector-memory failures reduce future candidate priority.
- Updated MCP runtime compatibility checks for supported Node LTS minimums.


## 0.7.7 — Chrome DevTools MCP primary browser engine

- Added Chrome DevTools MCP 1.10.1 as the primary semantic browser-control layer.
- MCP accessibility snapshots and UIDs now drive page inspection, named clicks, fills, navigation, typing and keys.
- Retained direct CDP as a fallback and for guarded uploads/screenshots/scrolling.
- Installer now provisions Node 22 LTS when needed and installs MCP into Fatir's private application data directory.
- Browser planner remains Gemma 4 31B Cloud in Auto/Cloud mode.

## 0.7.6 — Browser control model ownership fix

- Auto/Cloud browser tasks are now owned end-to-end by Gemma 4 31B Cloud instead of GPT-OSS 20B.
- Browser screenshots stay in the same multimodal tool loop; they are no longer reduced to a one-shot vision handoff and returned to 20B.
- Browser intent detection now includes WhatsApp, Gmail, upload/attachment/share, marketplace, and online-search wording.
- Cloud fallback/escalation respects browser ownership and returns to Gemma 4 for browser work.
- Explicitly selected model names remain explicit; Local-only never silently spends cloud quota.



## 0.7.5 — WhatsApp media preview verification fix

- Fixed image/video attachments stopping before Send when WhatsApp shows a real media preview without rendering the local filename.
- Media is now verified by the exact populated file input plus an active visual media preview; documents still require the filename to be visible.
- Kept attachment kind consistent through Send and post-send verification instead of hard-coding media as document later in the flow.
- Preserved the Forward/Share/Reply blacklist and no-guessing behavior.

## 0.7.4 — Rust format-string compile fix

- Escaped the JavaScript block braces added to the document-input scorer so the embedded script is valid inside Rust `format!()`.
- Removed the unused `pct` variable warning in adaptive-route context generation.
- Added static regression checks for the escaped document-scoring block.
- Carries forward the v0.7.3 WhatsApp attachment routing, Forward blacklist, direct Send targeting, and post-send verification changes unchanged.


## 0.7.3 — Verified WhatsApp file sending

- Fixed WhatsApp document attachment routing when the real file input uses `accept="*"`; media-only hidden inputs are now penalized for document uploads.
- WhatsApp no longer treats a successful CDP file-input assignment as proof that the requested file reached the chat; a real attachment preview is required before Send.
- Replaced coordinate-based attachment Send activation with a positively identified DOM target inside the active preview.
- Explicitly rejects Forward, Share, Reply, Delete, Cancel, Remove and Next controls; the old Enter fallback is removed from the WhatsApp attachment-send path.
- After Send, Fatir requires evidence of a new outgoing WhatsApp message before reporting success or writing a successful share-history entry.
- Added state logging for chat confirmation, file validation/injection, preview confirmation, Send activation and outgoing-message verification.


## 0.7.2 — Low-friction sharing + resilient Send recovery

- Removed redundant approval prompts for explicit web file uploads, WhatsApp shares, and email draft attachments.
- Destructive deletion, uninstall/removal, privileged/security changes, purchases, and stored credentials still require approval.
- Made browser text-control discovery understand aria-label, title, data-testid, data-icon, and clickable ancestors on reactive web apps.
- Reworked WhatsApp attachment Send detection to reacquire dynamic controls, handle icon-only buttons, and use a bounded Enter fallback only when the attachment preview is confirmed.
- Keeps Fatir's virtual cursor visible only while an action is active and verifies the page state after Send.

## 0.7.1 — Share UI compile fix

- Fixed the `ShareUiState` mutex guard lifetime error in `set_pending_share`.
- Handles a poisoned share-state mutex by recovering the inner value rather than failing the share handoff.

## 0.7.0 — Fatir Share
- Added a system-wide Fatir Share Sheet for any regular local file.
- Added Nemo right-click **Share with Fatir** integration for single or multi-file selections.
- Added deterministic WhatsApp sharing through Fatir-controlled Chrome/Chromium, with recent-chat discovery, contact search, visible Fatir cursor, direct CDP attachment, send verification, and no physical mouse takeover.
- Added Gmail draft creation with local attachments; Fatir intentionally leaves final Send to the user.
- Added smart clipboard sharing: images are copied as actual image data; other files are copied as `text/uri-list`; paths can be copied separately.
- Added local share history and cached WhatsApp recent-chat names (no message contents).
- Added Share buttons beside generated/found file resources in Fatir.
- Added **Share latest screenshot** and changed Fatir screenshots to `~/Pictures/screenshot/ksnip_YYYYMMDD-HHMMSS.png`; screenshots auto-copy as image data.
- Added command-line `fatir --share <files...>` handoff used by Nemo and external workflows.
- Added chat tools for generic WhatsApp file send, Gmail attachment draft, and latest screenshot resolution.


## 0.6.4

- Rebuilt webpage attachments as a generic atomic browser operation rather than a WhatsApp-specific flow.
- Fixed the root CDP handle bug: remote object IDs are now kept and consumed within the same DevTools websocket session instead of being reused across sessions.
- Added file-chooser interception for upload widgets that only expose an input after a button/menu action, so native dialogs do not steal Fatir's browser state.
- Web attachment now rescans dynamic DOM up to four times and can recover when React/Vue-style rerenders replace the file input.
- `browser_elements` now keeps semantic locators, and element click/fill operations can recover a replaced control after same-page DOM rerenders instead of immediately failing with a stale Fatir element ID.
- Updated agent guidance so `browser_upload_file` is the first operation for any website attachment/upload after the local file exists; manual attachment-menu clicking is now fallback behavior.

## 0.6.3

- Unified browser and desktop virtual pointers under one cursor-surface arbiter so both Fatir pointers cannot remain visible together.
- Added explicit cursor cleanup on completion, error, cancellation, approval completion, denial, and Take Over/Pause.
- Added automatic pointer idle-hide on both the browser overlay and Linux desktop overlay.
- Added `browser_upload_file`, a CDP-native local file upload path that avoids the native file chooser and prevents browser/desktop cursor desynchronization during WhatsApp Web and other attachment workflows.
- Browser/desktop surface switches now hide the previous cursor before control moves to the next surface.
- Expanded browser routing hints for WhatsApp, upload, attach, and attachment workflows.

## 0.6.2

- Added the **Vision Bridge**: when GPT-OSS, Nemotron, Fatir Router, or another text-only model encounters an image, screenshot, or rendered PDF page, Gemma 4 inspects the visual payload once and returns a compact factual handoff.
- The originally selected model immediately resumes control after the handoff; Gemma does not take over the conversation or run a second agent loop.
- Raw image payloads are removed from session history after bridging, so text-only chats continue normally instead of becoming permanently incompatible after one visual turn.
- Browser/desktop screenshots produced by Fatir tools are marked as generated visual context so they no longer replace the user's actual task when routing subsequent actions.
- Vision Bridge usage appears in the action trace and its token counts are included in efficiency reporting.
- If vision analysis fails, Fatir records the failure as context and keeps the conversation alive instead of breaking the chat.
- Local-only mode remains strict: it will never silently invoke the cloud Vision Bridge.

## 0.6.1
- Fixed desktop-control routing: desktop/window/application-control requests are now a first-class local intent instead of falling through to a cloud model.
- Added a zero-LLM Desktop Fast Path for listing controllable open desktop windows.
- Added a harmless deterministic desktop-control demo using Calculator + Fatir's independent pointer when explicitly requested.
- Exposed desktop AT-SPI tools to the 0.8B local router for desktop requests.
- Adaptive learning now records `fatir-router` as a local route instead of `other`.

# Fatir changelog

## 0.6.0
- Added **Fatir Computer Use 3.0** with an independent visible virtual pointer.
- Browser actions now visibly move Fatir's in-page pointer to the real DOM target, highlight it, and click/scroll through CDP without moving the user's physical mouse.
- Desktop AT-SPI actions now animate a separate transparent click-through Fatir pointer to the target's screen bounds before activation.
- Added a header **Take over / Resume** control; paused computer control blocks browser and desktop actions until resumed.
- Added GTK pointer-overlay runtime support (`python3-gi`, `python3-cairo`, `gir1.2-gtk-3.0`).
- Browser fill operations now focus fields through Fatir's virtual pointer before sending text; stored credentials remain outside model context.
- Existing DOM-first / accessibility-first routing, Fast Paths, local routing, adaptive learning and physical-mouse isolation remain intact.

## 0.5.2
- CPU/RAM/disk status is now a deterministic Fast Path: zero model turns, no Ollama model load.
- The local 0.8B router is capped at a 12-second response timeout instead of holding the UI.
- Auto/Cloud defaults to GPT-OSS 20B for lower quota usage; Gemma 4 31B is the default vision model.
- Starter cloud choices are always visible: GPT-OSS 20B/120B, Gemma 4 31B, Nemotron 3 Nano 30B, Nemotron 3 Super, and Nemotron 3 Ultra.
- Paid Qwen cloud vision is no longer hard-coded into Auto.

# Fatir v0.5.2

- Demotes the 4B local model from the default resident path.
- Adds `fatir-router` (Qwen3.5 0.8B) as the fast local router/tool selector.
- Auto mode escalates once to cloud when the router cannot safely map a task, rather than returning a no-access refusal or waiting on 4B.
- Adds a zero-model fast path and dedicated read-only tool for largest-folder queries, including hidden directories.
- Router context: 2K; output budget: 64; thinking off; temperature 0.
- Keeps `fatir-local` 4B available as explicit Local Deep mode.
- Adaptive learning continues to record fast-path/local/cloud outcomes and can prefer proven routes.

# Changelog

## 0.5.0

### Local-first intelligence
- Added `fatir-local` as Fatir's preferred Linux/system brain in Auto mode.
- Routine system, software, cleanup, file and service work stays local when possible.
- Local requests force thinking off, use a 4K working context, low temperature and a short output budget.
- Added **Auto / Local only / Cloud only** routing modes.
- Model picker now merges local Ollama models and cloud models instead of hiding local models when a cloud key is configured.
- Local model failures in Auto mode may escalate once to cloud; Local-only never silently uses cloud.
- Added `ollama/Fatir-Modelfile`; installer creates `fatir-local` automatically when `qwen3.5:4b` is already installed.

### Adaptive self-improvement
- Added a local adaptive learning engine that records task class, route, outcome, latency and model-token telemetry.
- Successful local routes are preferred after enough evidence; repeatedly unsuccessful routes can fall back to a better proven route in Auto mode.
- Repeated task classes generate reusable-routine suggestions rather than silently creating consequential automations.
- Added a Settings panel for adaptive learning, local-first routing, route learning, routine suggestions and reset.
- Adaptive learning never rewrites Fatir's source code, changes approval/security policies or stores credential values.

### Reliability
- Added an AH -> Fatir Cargo-cache compatibility bridge for stale absolute Tauri permission paths.
- Preserves the shared compilation cache rather than forcing a cold rebuild.

## 0.4.1
- Repaired AH -> Fatir Tauri cache migration handling.

## 0.4.0
- Added deterministic browser/Amazon fast paths and Fatir rebrand.
