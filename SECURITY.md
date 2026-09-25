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

## Local schedules and Scheduled Agent/Web Tasks

Fatir has two deliberately separate scheduling systems.

**Local schedules** are shell commands stored under the Fatir data directory and launched with systemd user timers. They do not create a future model/browser session and do not grant browser or headless permission.

**Scheduled Agent/Web Tasks** may wake Fatir and run a model later. Creating one is always approval-gated, and the approval summary includes its browser mode and the number of stored credentials being authorized. Browser access is scoped to the saved mode (`none`, `managed`, `current`, or explicitly approved `headless`). Only individually selected credential IDs may be reused without a second credential prompt. Secret values remain in the credential broker and are never added to model context or chat history.

Authenticated scheduled workflows must use the managed browser. Fatir intentionally does not inject stored credentials into the isolated headless browser. Authentication challenges such as OTP/authenticator codes, CAPTCHA, passkeys, or security keys must pause for human takeover; Fatir does not guess or bypass them.


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
