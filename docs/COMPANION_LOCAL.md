# Fatir Companion — local MVP

This branch proves the direct Android ↔ Linux path before any remote relay, public endpoint, mDNS discovery, or QR pairing is introduced.

## Architecture

```text
Android Companion
       │
       │ local Wi-Fi
       │ Bearer token
       ▼
Fatir LAN service :32145
       │
       ├── existing Fatir SharedState
       ├── chat / task engine
       ├── approve / deny
       ├── approved Linux roots
       └── streamed upload / download
```

The phone is a client. Fatir Desktop remains the brain.

## Current endpoints

- `GET /api/v1/health`
- `POST /api/v1/chat`
- `POST /api/v1/approve`
- `POST /api/v1/deny`
- `GET /api/v1/files/roots`
- `GET /api/v1/files/list?path=...`
- `GET /api/v1/files/download?path=...`
- `POST /api/v1/files/upload?directory=...`

All endpoints except health require:

```http
Authorization: Bearer <device-token>
```

The token is generated once and stored with user-only permissions at:

```text
~/.local/share/Fatir/companion/device-token
```

## Approved Linux file roots

The MVP exposes the user's home directory and the user's `/media/<user>` mount area when present. All requested paths are canonicalized and checked against those roots before file access.

Phone uploads default to:

```text
~/.local/share/Fatir/companion/uploads
```

A requested upload destination must also be an approved Linux directory.

## Desktop test

Checkout the branch and install:

```bash
git fetch origin
git checkout companion-local-mvp
chmod +x install.sh
./install.sh
```

Start Fatir, then:

```bash
chmod +x scripts/companion-info.sh
./scripts/companion-info.sh
```

Local health check:

```bash
curl http://127.0.0.1:32145/api/v1/health
```

Authenticated file-root check:

```bash
TOKEN="$(cat ~/.local/share/Fatir/companion/device-token)"
curl -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:32145/api/v1/files/roots
```

## Android test

Open `companion-android` in Android Studio or use the debug APK produced by the Companion Local MVP GitHub Actions workflow.

On the connection screen enter:

- the Linux machine's LAN IP, for example `192.168.1.20:32145`;
- the device token printed by `scripts/companion-info.sh`.

The app validates both the health endpoint and an authenticated endpoint before considering the phone connected.

## Security scope

This is intentionally a **trusted-LAN test build**. It uses a strong persistent bearer token but cleartext HTTP, so:

- do not expose or port-forward TCP 32145;
- do not use it on hostile/public Wi-Fi;
- do not use this transport for remote internet access.

Before remote access, the next milestone is encrypted pairing and transport, token rotation/revocation, Android Keystore-backed device identity, and a desktop pairing UI.
