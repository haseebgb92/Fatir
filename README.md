<div align="center">
  <img src="ui/assets/fatir-mark.png" alt="Fatir Arabic calligraphy logo" width="120" />

# Fatir

**A resident AI assistant for Linux that works beside you — not in another browser tab.**

Fatir is a light Tauri sidebar for Linux Mint/Cinnamon that can operate desktop apps, work with your active browser session, run terminal tasks, manage files and projects, remember useful context, and learn repeatable workflows.

**Current release:** v1.1.1 · **Primary target:** Linux Mint Cinnamon / X11 · **License:** MIT
</div>

---

## What Fatir is

Fatir lives in the Linux desktop as a resident sidebar. It is designed around one principle: **use the strongest local/deterministic control path first, then involve a model only when reasoning is actually needed.**

It combines:

- whole-desktop app control through AT-SPI, keyboard/window control, and an X11 visual fallback;
- Chrome DevTools MCP for semantic browser automation and active-session control;
- local-first model routing through Ollama, with cloud escalation for harder reasoning and visual/browser work;
- a verification-aware task engine that checks results before reporting success;
- persistent projects, terminal sessions, memory, schedules, background jobs, recovery state, and reusable Teach workflows;
- explicit approvals for destructive, privileged, credential-bearing, and other consequential actions.

Fatir is **not an IDE**. The UI is a narrow resident sidebar intended to stay beside Chrome, Android Studio, Nemo, GIMP, VLC, or any other Linux app you are using.

## Highlights

### Resident sidebar

- Light 420 px sidebar by default
- Left or right screen placement
- 360–520 px adjustable width
- Pin / auto-hide behavior
- Cinnamon panel launcher
- Command Center, chat, task trace, settings, memory, schedules, Teach Mode, and approvals inside the same sidebar

### Linux app control

Fatir discovers graphical applications and running windows, then chooses the least fragile control layer that works:

```text
AT-SPI semantic controls
        ↓ fallback
Window / keyboard control
        ↓ fallback
Visual X11 control
```

This allows Fatir to work with GTK, Qt, Electron, JetBrains, media apps, utilities, file managers, and custom-rendered apps without treating one application as a special case.

### Browser control

Chrome DevTools MCP is the primary semantic browser engine.

Fatir supports two different browser meanings:

- **Managed browser:** Fatir may use its own controlled Chrome session.
- **Active/current browser session:** Fatir attaches to the Chrome instance and tabs you are already using. It must not silently substitute a different browser session if attachment fails.

For Chrome 144+, active-session attachment uses Chrome DevTools MCP auto-connect. Enable remote debugging once in your normal Chrome at:

```text
chrome://inspect/#remote-debugging
```

Browser tasks in Auto/Cloud mode are routed to the vision-capable browser planner rather than the lightweight local router.

### Verified task execution

Substantial work follows:

```text
UNDERSTAND → PLAN → ACT → VERIFY → RECOVER / COMPLETE
```

A successful click, command, or tool call is not automatically considered task success. Browser and desktop mutations are expected to obtain post-action evidence before completion.

### Local-first routing

Typical routing order:

1. **Fast Path** — deterministic built-in tool, zero LLM calls when possible.
2. **Local Router** — small local Ollama model for simple intent/tool routing.
3. **Local Deep** — larger local model when useful.
4. **Cloud escalation** — for difficult reasoning, visual understanding, or browser work.

Explicit `Local only` and `Cloud only` modes are also available.

### Memory, Teach, and automation

- browser and desktop action memory;
- persistent project context and logical terminal sessions;
- Teach Mode 3.0 for recording semantic reusable workflows;
- systemd-user background jobs and local schedules;
- recovery classification and repeated-failure loop guards;
- risk-aware approvals and stored-credential isolation.

Headless browser operation is **explicit-only**. Saying “background” does not grant headless browser permission.

## Installation

### Recommended: clone the repository

```bash
git clone https://github.com/haseebgb92/Fatir.git
cd Fatir
chmod +x install.sh
./install.sh
```

The installer provisions the local desktop runtime required by Fatir, including accessibility, window/input control, screenshots, launcher integration, PolicyKit/D-Bus support, keyring integration, PDF/archive helpers, and the Tauri build dependencies.

It also uses a shared Cargo target cache at:

```text
~/.cache/Fatir/cargo-target
```

so future builds can reuse compiled Rust dependencies.

### Re-run desktop accessibility setup

```bash
./scripts/setup-desktop-access.sh
```

This helper uses normal `sudo` prompts because **you** are running the script manually. Fatir's internal privileged tool uses PolicyKit instead and never asks for your sudo password in chat.

## First-run smoke test

After installation, these are useful checks:

```text
Check your local desktop capabilities.
```

Chrome should not open for that request.

```text
Can you operate installed apps?
```

Fatir should report whole-desktop readiness rather than a single app-specific diagnostic.

```text
Open Nemo and go to Downloads.
```

Fatir should verify the resulting visible state.

```text
In my current Chrome session, go to the third tab and tell me what is on the page.
```

Fatir should remain attached to the Chrome session you are already using.

## Data and privacy

Fatir stores application data under:

```text
~/.local/share/Fatir
```

Important boundaries:

- stored credential values stay in Linux Secret Service / keyring;
- passwords, OTPs, API keys, recovery codes, and payment data are not intentionally placed into model context;
- browser network diagnostics redact sensitive headers and secret-like values;
- destructive, privileged, uninstall/remove/wipe, and stored-credential actions remain approval-gated;
- local desktop/file/system work is prevented from silently launching a browser;
- headless browser tools are unavailable unless the current request explicitly asks for headless execution.

See [SECURITY.md](SECURITY.md) for more detail.

## Development

### Validate the source

```bash
./scripts/validate.sh
```

### Rust check

```bash
cd src-tauri
cargo check
```

### Build a Debian package

```bash
cargo install tauri-cli --version '^2' --locked
cargo tauri build --bundles deb
```

GitHub Actions also runs validation, `cargo check`, and a Linux `.deb` build on pushes, pull requests, tags, and manual runs.

## Repository layout

```text
Fatir/
├── src-tauri/          Rust/Tauri application and agent runtime
│   └── src/            browser, desktop, tasks, memory, recovery, etc.
├── ui/                 sidebar frontend
├── cinnamon-applet/    Cinnamon panel launcher
├── nemo-actions/       Nemo integration
├── ollama/             local router/model definitions
├── scripts/            validation and desktop setup helpers
├── .github/workflows/  Linux CI/build workflow
├── install.sh
└── uninstall.sh
```

For a deeper implementation overview, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Release notes

- [CHANGELOG.md](CHANGELOG.md) — release-by-release history
- [V1-RELEASE.md](V1-RELEASE.md) — V1 invariants and smoke tests

## Current platform scope

Fatir's strongest target today is **Linux Mint Cinnamon on X11**. The semantic accessibility layers are portable across many Linux desktops, but the final coordinate-based visual fallback is intentionally X11-specific rather than guessing on unsupported Wayland environments.

## License

MIT. See [LICENSE](LICENSE).
