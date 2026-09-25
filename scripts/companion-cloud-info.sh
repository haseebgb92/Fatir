#!/usr/bin/env bash
set -euo pipefail

FILE="${XDG_DATA_HOME:-$HOME/.local/share}/Fatir/companion/relay.json"

if [[ ! -f "$FILE" ]]; then
  echo "Fatir cloud relay is not configured."
  echo "Run: ./scripts/companion-relay-configure.sh https://your-relay-domain"
  exit 1
fi

python3 - "$FILE" <<'PY'
import json, sys
p=sys.argv[1]
with open(p, encoding="utf-8") as f:
    c=json.load(f)
url=c["url"].rstrip("/")
http=url.replace("wss://","https://",1).replace("ws://","http://",1)
print("Fatir Cloud Companion")
print()
print("Enabled:", c.get("enabled", False))
print("Relay:", url)
print("Device ID:", c["device_id"])
print("Cloud Companion address:", f'{http}/d/{c["device_id"]}')
print("Relay device token:", c["token"])
PY
