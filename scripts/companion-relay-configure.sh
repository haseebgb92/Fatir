#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 https://relay.example.com"
  exit 1
fi

RELAY_URL="${1%/}"
case "$RELAY_URL" in
  https://*|http://*|wss://*|ws://*) ;;
  *)
    echo "Relay URL must start with https://, http://, wss://, or ws://"
    exit 1
    ;;
esac

DIR="${XDG_DATA_HOME:-$HOME/.local/share}/Fatir/companion"
FILE="$DIR/relay.json"
mkdir -p "$DIR"

DEVICE_ID="$(cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen)"
TOKEN="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"

cat > "$FILE" <<JSON
{
  "enabled": true,
  "url": "$RELAY_URL",
  "device_id": "$DEVICE_ID",
  "token": "$TOKEN"
}
JSON
chmod 600 "$FILE"

HTTP_URL="$RELAY_URL"
HTTP_URL="${HTTP_URL/wss:\/\//https:\/\/}"
HTTP_URL="${HTTP_URL/ws:\/\//http:\/\/}"

echo "Fatir cloud relay configured."
echo
echo "Relay: $RELAY_URL"
echo "Device ID: $DEVICE_ID"
echo "Cloud Companion address: $HTTP_URL/d/$DEVICE_ID"
echo "Relay device token: $TOKEN"
echo
echo "Restart Fatir, then use the Cloud Companion address + relay device token on Android."
