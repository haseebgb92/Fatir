# Fatir Architecture

Fatir is a resident Linux assistant built as a Tauri application with a Rust runtime and a narrow HTML/CSS/JavaScript sidebar frontend.

## Control surfaces

Fatir treats each environment as a distinct control surface and keeps routing state so an ongoing browser task does not suddenly become a local desktop task, or vice versa.

### Desktop

Preferred order:

1. AT-SPI semantic accessibility tree
2. window-scoped keyboard/window operations
3. visual inspection
4. X11 visual-coordinate fallback

The visual fallback is intentionally the last resort.

### Browser

Chrome DevTools MCP is the primary semantic browser layer. Fatir supports both a managed browser profile and an active/current Chrome session. Active-session mode must not fall back to the managed profile if attachment fails.

### Terminal and filesystem

Local shell, project, file, and background-job operations are separate from browser execution. Browser startup is guarded at both routing and execution layers.

## Task lifecycle

Substantial work follows:

```text
UNDERSTAND → PLAN → ACT → VERIFY → RECOVER / COMPLETE
```

Persistent task state stores checkpoints, evidence, blockers, attempt counts, and resume state. Browser/desktop mutations are expected to be followed by a read-only observation before success is reported.

## Model routing

Fatir aims to minimize unnecessary model use:

- deterministic Fast Path first;
- small local router for simple local intent/tool routing;
- larger local model when needed;
- cloud escalation for difficult reasoning, visual understanding, and browser control.

Browser work in Auto/Cloud mode is owned by the stronger vision-capable browser planner.

## Memory and learning

Separate stores track project context, terminal sessions, browser target memory, adaptive routing history, routines, taught workflows, schedules, jobs, and task state.

Learned browser selectors are semantic rather than coordinate-based. Teach Mode records reusable semantic actions and verification hints while excluding credentials and transient browser identifiers where possible.

## Safety boundaries

- destructive and privileged operations are approval-gated;
- stored credential values are retrieved only at execution time and are not intentionally sent to the model;
- browser diagnostics redact sensitive headers and secret-like values;
- headless browser execution requires explicit current-turn intent;
- local desktop/system work cannot silently wake Chrome;
- active-browser mode cannot silently switch to a different managed Chrome session.
