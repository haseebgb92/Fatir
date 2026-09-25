# Fatir Cloud Relay

The relay gives Fatir Companion remote access without opening an inbound port on the Linux PC.

## Traffic flow

```text
Android ── HTTPS ──> Relay
                     │
                     │ WSS
                     ▼
              Fatir Desktop
                     │
                     ▼
              local :32145 API
```

Fatir Desktop initiates the WebSocket connection outbound. The relay forwards the existing Companion API and does not run an agent or model.

## Security model

- LAN and relay credentials are separate.
- The relay token is a random per-device secret.
- Deploy the relay only behind HTTPS/WSS.
- The relay accepts API traffic only for currently connected devices.
- The relay does not persist message bodies or file contents.
- This MVP uses TLS transport security; application-level end-to-end encryption is a later hardening milestone.

## Limits

`FATIR_RELAY_MAX_BODY_MB` defaults to 32 MB. That is enough for chat and ordinary attachments. Large/resumable file transfers will use a chunked protocol in a later build.

## Run locally

```bash
cd relay-server
cargo run
```

The default port is `8787`.

## Docker

```bash
docker build -t fatir-relay .
docker run --rm -p 8787:8787 fatir-relay
```

For public use put it behind Caddy, Nginx, Traefik, or another TLS terminator and use a domain such as `https://relay.example.com`.
