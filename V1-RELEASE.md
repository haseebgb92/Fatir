# Fatir v1.2.0 Release Gate

Fatir V1 is the consolidation release that turns browser, Linux desktop, terminal, filesystem, projects, background jobs, schedules, memory and taught routines into one verified execution system.

## Core invariants

- Substantial work follows **UNDERSTAND → PLAN → ACT → VERIFY → RECOVER/COMPLETE**.
- A browser/desktop mutation cannot be the last step before success; Fatir must inspect the resulting state.
- Multi-step persistent tasks require a verification checkpoint with concrete evidence before completion.
- Local Linux/application work cannot start a browser unless the current request is genuinely browser-scoped.
- Headless browser operation is available only when the current request explicitly says `headless`; `background` never implies headless.
- Stored credentials never enter model context.
- Destructive, uninstall/remove/wipe, privileged/system and credential-bearing actions remain approval-gated.
- Local shell schedules remain command-only. Scheduled Agent/Web Tasks may wake Fatir later, but browser mode is granted explicitly per schedule and stored credentials are reusable only when individually pre-authorized at schedule creation.
- Authentication challenges such as OTP/authenticator, CAPTCHA, passkey, or security-key checks must pause for human takeover; Fatir never guesses or bypasses them.

## First-run smoke test

After `./install.sh` succeeds, run these in order:

1. `Check your local desktop capabilities.`
   - Desktop Doctor should report the local runtime; Chrome must not open.
2. `Can you operate installed apps?`
   - Must remain desktop/local. Chrome must not open.
3. `Open Nemo and go to Downloads.`
   - Verify the visible Nemo state after navigation.
4. `Open Calculator and calculate 1250 * 17.`
   - Exercise semantic/keyboard desktop control and verification.
5. `Open Amazon, search for dog leash, and tell me the first organic result.`
   - Visible Chrome should open only here; browser planner should use Gemma 4 31B + Chrome DevTools MCP.
6. Send a local image to a WhatsApp contact.
   - File preview must be verified, Forward must never satisfy Send, and a new outgoing message must be observed before success.
7. `Inspect this project and remember it.` from a development folder.
   - Verify project kind/Git context and project registry.
8. Create a terminal session, change its cwd, run `pwd`, then ask for its history.
   - Cwd/history should persist across turns.
9. Teach a harmless multi-step workflow, save it, then inspect the stored routine.
   - Recorded steps should be semantic and contain verification hints, never coordinates/secrets.
10. `Remind me in 10 minutes with notify-send that the test passed.`
    - A persistent systemd user timer should appear in scheduled-job state.
11. `Run this command in the background: sleep 20.`
    - Must create a local background job; it must **not** start headless Chrome.
12. `Run this headless: open example.com and read the title.`
    - Only this explicit wording may expose the isolated headless browser session.

## Release validation performed before packaging

The source package is required to pass:

- Fatir static/regression validator.
- Shell syntax checks for installer/uninstaller/runtime setup scripts.
- JavaScript syntax validation for the Tauri UI.
- JSON/TOML parsing and version consistency checks.
- Independent Rust lexical delimiter/string/comment scan across all source modules.
- Browser/local isolation, explicit-headless, WhatsApp Send, Teach, runtime-dependency and external-command regression guards.
- Persistent schedule unit syntax verification with `systemd-analyze verify` when available.
- Fresh ZIP extraction followed by the same validation suite.

The release environment used to assemble this source does not contain a Rust toolchain and cannot download one. Therefore the user's `./install.sh` on Linux Mint is the first real Cargo/Tauri compilation and live hardware/application integration test.


## v1.0.4 hotfix smoke tests

- `Check your local desktop capabilities.` must return `Fatir local fast path`, use zero Ollama model calls, and must not launch Chrome.
- With `fatir-router` absent but `fatir-local`/`qwen3.5:4b` available, simple local work must stay on the local model.
- A direct call chain reaching browser startup without browser intent must fail with the browser-start permission error before spawning Chrome.
- Start a browser inbox workflow, perform several successful browser actions, then say `rather searching how about you click all these from inbox daily and delete`; it must remain browser-scoped and Gemma-owned instead of switching to Local Router.
- After that browser workflow, say `now open the local file in Nemo`; browser continuity must end immediately and the turn must use the local/desktop surface.
- A blocked browser tool attempt on a truly local turn must not establish browser continuity for the next message.

## v1.1.1 sidebar UI smoke tests

1. Click the Cinnamon Fatir icon: one light sidebar opens on the configured monitor edge; no extra app window appears.
2. Click `×`: Fatir hides instead of quitting. Click the Cinnamon icon again: the same resident process reopens.
3. Leave the panel unpinned and click another app: Fatir auto-hides. Pin it and repeat: Fatir remains visible.
4. Settings → Sidebar appearance: switch Right/Left and move width through 360–520 px. The panel must stay inside the monitor and the setting must survive hide/reopen in the same app session and persist through frontend local storage across restarts.
5. Verify chat send, attachments, screenshots, model picker, Command Center, Activity, Share and Settings all remain functional at 420 px width.
6. Run the v1.0.4 browser/local routing smoke tests unchanged; this release must not alter agent routing or browser/desktop safety behavior.
