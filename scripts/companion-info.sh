#!/usr/bin/env bash
set -euo pipefail

TOKEN_FILE="${XDG_DATA_HOME:-$HOME/.local/share}/Fatir/companion/device-token"
IP="$(hostname -I 2>/dev/null | awk '{print $1}')"

echo "Fatir Companion local MVP"
echo
echo "Linux IP: ${IP:-unavailable}"
echo "Port: 32145"
echo "Address: http://${IP:-127.0.0.1}:32145"
echo

if [[ -f "$TOKEN_FILE" ]]; then
  echo "Device token:"
  cat "$TOKEN_FILE"
else
  echo "Device token not created yet."
  echo "Start Fatir once from the companion-local-mvp build, then run this script again."
fi
