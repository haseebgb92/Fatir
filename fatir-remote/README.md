# Fatir Remote

Fatir Remote is a separate Android streaming client generated from **Moonlight Android 12.2**
at pinned upstream commit:

`b48494cb96bff23d8886c4775cc4f39a1075495d`

It is intentionally separate from the MIT-licensed Fatir desktop and Companion applications.
Fatir Remote is distributed under **GNU GPLv3**, matching Moonlight Android and
moonlight-common-c.

## Security model

- Sunshine performs screen capture, encoding, session authentication, and host authorization.
- Fatir Remote uses Moonlight's normal one-time PIN + client certificate pairing.
- Sunshine web-admin username/password are never stored in Fatir Companion or Fatir Remote.
- Pairing keys stay in Fatir Remote's private Android application storage.
- Use the existing Tailscale address when the host is not discoverable on the local LAN.
- No Fatir VNC, x11vnc, FFmpeg streaming, or extra remote-control port is required.

## Fatir modifications

The generated client keeps Moonlight's networking/decoder/pairing implementation intact and
only applies a small UI/branding patch:

- application ID: `com.fatir.remote`
- Fatir branding
- top hamburger with Keyboard / Stats / Disconnect controls
- dedicated right-edge smooth scroll rail

The reproducible patch is in `apply-fatir-remote.py`.

## Build

GitHub Actions clones the pinned Moonlight source recursively, applies the Fatir patch, installs
the matching Android SDK/NDK, and builds the non-root debug APK.
