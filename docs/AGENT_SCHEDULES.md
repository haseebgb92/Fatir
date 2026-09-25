# Scheduled Agent / Web Tasks

Fatir 1.2 adds model-driven schedules alongside the existing local command scheduler.

## Two different schedulers

- `scheduled_job_*`: local shell/reminder jobs only.
- `agent_schedule_*`: persistent Fatir agent tasks that may reason, use approved browser mode, and use individually approved stored credentials.

Agent schedules are backed by systemd user timers with `Persistent=true`, so a missed timer can run when the Linux user session returns.

## Browser authorization

Every agent schedule stores one browser mode:

- `none`: no browser permission.
- `managed`: use Fatir's persistent managed visible browser profile.
- `current`: use only the user's already-running browser session.
- `headless`: use the isolated headless browser. This is only available when explicitly selected and approved at schedule creation.

The creation approval shows the selected browser mode and how many stored credentials are being authorized.

## Credentials

Fatir lists only credential metadata to the model. Secret values stay inside the Linux keyring.

A scheduled task may reuse only the credential IDs explicitly attached to that schedule at creation. Password filling is performed inside Fatir and the secret is never returned to model context or chat history.

For login workflows, Fatir can:

- use saved username/password credentials;
- choose a normal sign-in flow;
- choose "Continue/Sign in with Google" when requested or appropriate;
- reuse the persistent logged-in managed browser profile.

If the site requires OTP/authenticator verification, CAPTCHA, passkey, security key, or another human verification step, Fatir pauses browser control for the user. It must not guess or bypass the challenge.

## Chat history

User and assistant turns are stored locally in:

`~/.local/share/Fatir/chat-history.json`

Desktop Fatir and Companion read the same history through the authenticated Companion API. A chat opened on the phone can continue with the same session ID on Linux, and vice versa.

## Examples

Ask Fatir naturally:

`Every weekday at 9 AM, check Gmail for new important messages and summarize anything that needs my attention. Use the managed browser.`

`Every Monday at 10 AM, research a topic, write an SEO article, prepare the image, log into WordPress with my approved credential, publish it, verify the public URL, and report the result.`

`Every evening, check my website dashboard and report anything that looks wrong. Do not make changes.`

Fatir should inspect available credentials before proposing a schedule that needs login, then request one creation approval showing the browser and credential grants.

## Verification

A scheduled task does not count as complete merely because a button was clicked. Existing Fatir verification rules remain active. For example a WordPress publish workflow should verify the final post state/public URL before reporting success.
