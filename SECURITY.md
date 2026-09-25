# Fatir Security Model

Fatir can act on the local computer. Its default design is low-friction for actions the user explicitly requested, while preserving confirmation boundaries for destructive, privileged, security-sensitive, purchase, uninstall/removal, and stored-credential operations.

## Privilege boundaries

- Root-required work goes through PolicyKit (`pkexec`). Fatir does not receive the sudo/root password.
- Permanent deletion such as emptying Trash requires explicit approval.
- Normal cleanup moves selected items to desktop Trash first.
- Generic shell execution requires approval and is not treated as automatically reversible.

## Credentials

Credential values are stored in Linux Secret Service through the Rust `keyring` crate. Fatir's local JSON metadata contains only IDs/labels/account labels. The model receives credential IDs, not secret values. Approved credential tools retrieve the value inside the Rust process and inject it directly into an approved browser field or child-process environment. Exact secrets are scrubbed from captured process output where possible.

Legacy AH keyring entries are read only for migration and are copied forward to the Fatir service name.

## Browser automation

Fatir's visible controlled browser uses Chrome DevTools MCP as the primary semantic layer, with direct Chrome DevTools Protocol fallbacks for guarded operations such as uploads and low-level recovery. Normal local Linux/files/terminal/desktop tasks are execution-blocked from starting a browser indirectly. Headless browser tools are opt-in only when the current request explicitly says `headless`; `background` wording alone does not authorize headless execution. Browser network/console diagnostics redact authorization/cookie/token-like values before they reach model context. An explicit user instruction to upload/share a chosen file or send a specific message is treated as authorization for that routine action and does not trigger a redundant second approval card. Purchases, remote deletion, account/security changes, or content Fatir inferred rather than the user explicitly requested still require confirmation.

## Desktop automation

Whole-desktop actions prefer Linux AT-SPI semantic controls, then window-scoped keyboard control. For custom-rendered X11 applications that expose inadequate accessibility metadata, Fatir may use a screenshot-relative X11 visual fallback for a requested click/drag/scroll. That fallback records the real pointer position, performs the minimum synthetic input, and restores the original pointer position immediately afterward. It is not enabled as a generic Wayland pointer bypass. The user can pause computer control from the Fatir header and take over. Destructive GUI intentions remain approval-gated even when invoked through keyboard or visual fallback.


## V1 execution and verification

Substantial work is tracked through UNDERSTAND → PLAN → ACT → VERIFY → RECOVER/COMPLETE. Browser and desktop GUI mutations create a verification obligation; Fatir should inspect resulting state before reporting success. Repeated identical failures are loop-guarded and classified so recovery changes strategy rather than repeating blind actions. Persistent project, terminal, task, routine, schedule, and action-memory records are advisory operational state and never override approval/security rules.

## Schedules

Fatir has two deliberately separate scheduling layers.

**Local schedules** are deterministic shell commands/reminders stored under the Fatir data directory and launched with persistent systemd user timers. They never create a model/browser session and never grant browser or headless permission.

**Agent schedules** are explicitly authorized future Fatir runs. Their schedule record stores the task prompt, browser execution mode, verification requirement, and only the IDs of credentials the user chose to authorize for that schedule. Credential values remain in Linux Secret Service and are retrieved only inside the Rust execution process. A scheduled run cannot expand that authorization to other credentials and does not bypass destructive, administrator, purchase, account/security, or unrelated sensitive approvals.

Browser execution mode is fixed when the schedule is created: normal agent, visible managed browser, current/active browser only, or explicitly authorized isolated headless browser. Headless permission on one schedule does not grant headless access to other turns or schedules.

Authenticator/TOTP codes, security keys, CAPTCHAs, phone approvals, recovery steps, and unusual login verification are human checkpoints. Fatir pauses browser automation and surfaces the blocker instead of guessing, storing, or sending those values through normal model/chat history.

## Fast Paths

Zero-model Fast Paths are deterministic local skills, not expanded permissions. They still follow the same operation boundaries. The Amazon research fast path is read-only web navigation plus local CSV creation. The Google-login fast path only activates the explicitly requested login control and stops at credential/security verification boundaries.

## Rollback

Rollback is best-effort rather than a filesystem snapshot. Fatir can restore checkpointed files and undo supported moves/new installs. External applications and arbitrary shell commands may perform changes that cannot be reconstructed automatically, so important known configuration files should be checkpointed first.

## Local data

Operational state lives under:

```text
~/.local/share/Fatir/
```

Background activity learning can be disabled or cleared in Settings. Passwords, OTPs, recovery codes and keyring secret values must not be written to that data store.

## Adaptive self-improvement boundaries (v0.5.0)

Fatir's adaptive engine learns local outcome statistics and repeated workflow patterns. It may influence Auto-mode routing and suggest routines, but it is not allowed to rewrite its own executable/source, change approval levels, bypass credential protections, or autonomously promote destructive actions. Adaptive records contain route/task telemetry, not secret values, and can be reset from Settings.

## Fatir Share

Sharing a local file to WhatsApp or an email draft deliberately sends local data to a remote service. When the user explicitly asks Fatir to share/upload that file, or uses the Share Sheet directly, that is treated as authorization and Fatir does not ask a second time. Fatir Share stores only minimal local operational history (destination, recipient label, file paths, timestamp, status) and a cache of recent WhatsApp chat names; it does not store message contents or browser credentials.
