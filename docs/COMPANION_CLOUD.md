# Fatir Companion — Cloud Relay MVP

The cloud relay preserves the working LAN Companion API while allowing the phone to reach Fatir away from home.

## Design

```text
Same Wi-Fi
Android ───────────────> Fatir :32145

Away from home
Android ── HTTPS ──> Fatir Relay ── WSS ──> Fatir Desktop ── localhost:32145
```

The Linux PC never needs a router port-forward. Fatir opens the relay connection outbound.

## Important security boundary

The local Companion token and relay token are separate.

The cloud MVP is secured by HTTPS/WSS plus a random per-device relay token. The relay does not persist request bodies, responses, or file payloads. Application-level end-to-end encryption is planned before calling the remote transport production-hardened.

## File limit

The first relay tunnel supports request/response bodies up to 32 MB by default. This covers chat and ordinary files. Large files need the upcoming resumable chunked transfer protocol.

## 1. Deploy the relay

Build `relay-server` on an always-on server/container:

```bash
cd relay-server
docker build -t fatir-relay .
docker run -d --restart unless-stopped --name fatir-relay -p 8787:8787 fatir-relay
```

Put port 8787 behind an HTTPS/WSS reverse proxy. Do not expose a cleartext relay for internet use.

A public relay URL should look like:

```text
https://relay.example.com
```

## 2. Configure Linux Fatir

```bash
chmod +x scripts/companion-relay-configure.sh scripts/companion-cloud-info.sh
./scripts/companion-relay-configure.sh https://relay.example.com
```

Restart Fatir. It will reconnect automatically after network interruptions.

Show the mobile credentials again with:

```bash
./scripts/companion-cloud-info.sh
```

## 3. Connect Android

Use the printed **Cloud Companion address**, for example:

```text
https://relay.example.com/d/550e8400-e29b-41d4-a716-446655440000
```

Use the printed **Relay device token**, not the LAN device token.

The same Android API works through both local and cloud paths.

## Next hardening

- QR pairing and relay credential transfer
- Android Keystore storage
- local-first/cloud-fallback automatic routing
- device revoke/rotate
- app-level end-to-end encryption
- resumable chunked file transfer
- asynchronous task IDs/status streaming
