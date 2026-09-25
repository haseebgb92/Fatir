#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Preserve the Rust/Tauri cache from AH so the rename does not force a cold rebuild.
OLD_CACHE="$HOME/.cache/AH/cargo-target"
NEW_CACHE="$HOME/.cache/Fatir/cargo-target"
if [[ -d "$OLD_CACHE" && ! -e "$NEW_CACHE" ]]; then
  mkdir -p "$(dirname "$NEW_CACHE")"
  mv "$OLD_CACHE" "$NEW_CACHE"
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$NEW_CACHE}"
mkdir -p "$CARGO_TARGET_DIR"

# Compatibility bridge for stale Tauri build metadata created before the AH -> Fatir rename.
# This preserves the expensive shared Cargo cache without leaving broken absolute paths.
mkdir -p "$HOME/.cache/AH"
if [[ ! -e "$HOME/.cache/AH/cargo-target" ]]; then
  ln -s "$NEW_CACHE" "$HOME/.cache/AH/cargo-target" 2>/dev/null || true
fi

echo
echo "=========================================="
echo " Fatir Personal Assistant — installer"
echo "=========================================="
echo

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "Fatir currently targets Linux Mint/Ubuntu." >&2
  exit 1
fi

ARCH="$(uname -m)"
if [[ "$ARCH" != "x86_64" && "$ARCH" != "amd64" ]]; then
  echo "This build path is currently validated for x86_64. Detected: $ARCH" >&2
  exit 1
fi

DEPS=(
  # Build/runtime foundation
  build-essential curl wget file git pkg-config libssl-dev
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev

  # Local desktop control + app discovery
  at-spi2-core python3 python3-gi python3-cairo python3-pyatspi
  gir1.2-atspi-2.0 gir1.2-gtk-3.0 libatk-wrapper-java libatk-wrapper-java-jni
  xdotool wmctrl xclip x11-utils xauth gnome-screenshot scrot
  xdg-utils desktop-file-utils libglib2.0-bin libgtk-3-bin
  dbus-x11 libnotify-bin policykit-1

  # Local files, credentials, app packaging and content helpers
  gvfs-backends shared-mime-info gnome-keyring
  flatpak poppler-utils unzip tar procps psmisc
)
MISSING=()
for pkg in "${DEPS[@]}"; do
  dpkg -s "$pkg" >/dev/null 2>&1 || MISSING+=("$pkg")
done

if ((${#MISSING[@]})); then
  echo "[1/9] Installing ${#MISSING[@]} missing system dependencies…"
  echo "       ${MISSING[*]}"
  while sudo fuser /var/lib/dpkg/lock-frontend /var/lib/apt/lists/lock >/dev/null 2>&1; do
    echo "       Mint is using APT; waiting 5 seconds…"
    sleep 5
  done
  sudo apt update
  sudo apt install -y "${MISSING[@]}"
else
  echo "[1/9] System dependencies already present — skipping APT."
fi

# Enable the generic desktop-control stack for this user. Dependency installation
# was already handled above, so this pass only enables accessibility, configures
# existing JetBrains-family profiles, and verifies the local AT-SPI/X11 layers.
FATIR_SKIP_APT=1 "$ROOT/scripts/setup-desktop-access.sh" || echo "       Desktop accessibility setup needs attention; Fatir desktop_doctor can diagnose it after install."

# Chrome DevTools MCP is Fatir's primary semantic browser-control engine.
# It requires Node 20.19+ (or Node 22.12+); install a system Node 22 LTS when the host is incompatible.
NODE_OK=0
SYSTEM_NODE="/usr/bin/node"
if [[ -x "$SYSTEM_NODE" ]]; then
  NODE_VER="$("$SYSTEM_NODE" -p "process.versions.node" 2>/dev/null || true)"
  NODE_MAJOR="${NODE_VER%%.*}"
  NODE_REST="${NODE_VER#*.}"; NODE_MINOR="${NODE_REST%%.*}"
  if [[ "$NODE_MAJOR" =~ ^[0-9]+$ && "$NODE_MINOR" =~ ^[0-9]+$ ]]; then
    if (( (NODE_MAJOR == 20 && NODE_MINOR >= 19) || (NODE_MAJOR == 22 && NODE_MINOR >= 12) || NODE_MAJOR >= 24 )); then NODE_OK=1; fi
  fi
fi
if (( ! NODE_OK )); then
  echo "[2/9] Installing Node.js 22 LTS for Chrome DevTools MCP…"
  TMP_NODE_SETUP="$(mktemp)"
  curl -fsSL https://deb.nodesource.com/setup_22.x -o "$TMP_NODE_SETUP"
  sudo -E bash "$TMP_NODE_SETUP"
  rm -f "$TMP_NODE_SETUP"
  sudo apt install -y nodejs
else
  echo "[2/9] System Node.js ${NODE_VER} is compatible with Chrome DevTools MCP."
fi

# Use the system Node/npm pair so Fatir launched from Cinnamon/autostart does not depend on an interactive nvm shell.
SYSTEM_NODE="/usr/bin/node"
NPM_BIN="/usr/bin/npm"
if [[ ! -x "$SYSTEM_NODE" || ! -x "$NPM_BIN" ]]; then
  echo "Compatible system node/npm was not installed correctly." >&2
  exit 1
fi

MCP_VERSION="1.10.1"
MCP_ROOT="$HOME/.local/share/Fatir/chrome-devtools-mcp"
MCP_CURRENT=""
if [[ -f "$MCP_ROOT/node_modules/chrome-devtools-mcp/package.json" ]]; then
  MCP_CURRENT="$("$SYSTEM_NODE" -p "require('$MCP_ROOT/node_modules/chrome-devtools-mcp/package.json').version" 2>/dev/null || true)"
fi
if [[ "$MCP_CURRENT" != "$MCP_VERSION" || ! -x "$MCP_ROOT/node_modules/.bin/chrome-devtools" ]]; then
  echo "       Installing Chrome DevTools MCP ${MCP_VERSION} into Fatir's private runtime…"
  mkdir -p "$MCP_ROOT"
  "$NPM_BIN" install --prefix "$MCP_ROOT" --no-audit --no-fund "chrome-devtools-mcp@${MCP_VERSION}"
else
  echo "       Chrome DevTools MCP ${MCP_VERSION} already installed."
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "[3/9] Installing Rust toolchain…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
else
  echo "[3/9] Rust already installed."
fi

export PATH="$HOME/.cargo/bin:$PATH"

# AH -> Fatir cache migration repair.
# Tauri 2 build metadata contains absolute paths to generated permission files.
# Reusing a target directory that was physically moved from ~/.cache/AH to
# ~/.cache/Fatir can therefore leave otherwise-valid cached artifacts pointing
# at the old location. Clean only Tauri's path-sensitive artifacts once while
# preserving the rest of Cargo's expensive dependency cache.
CACHE_REPAIR_MARKER="$HOME/.cache/Fatir/.tauri-cache-paths-repaired-v1"
if [[ -d "$CARGO_TARGET_DIR" && ! -f "$CACHE_REPAIR_MARKER" ]]; then
  STALE_CACHE=0
  if [[ "$CARGO_TARGET_DIR" == "$NEW_CACHE" ]]; then
    if [[ ! -d "$OLD_CACHE" ]]; then
      STALE_CACHE=1
    elif grep -RIl --exclude='*.rlib' --exclude='*.rmeta' --exclude='*.so' \
      "$HOME/.cache/AH/cargo-target" \
      "$CARGO_TARGET_DIR/release/build" "$CARGO_TARGET_DIR/release/.fingerprint" \
      >/dev/null 2>&1; then
      STALE_CACHE=1
    fi
  fi

  if (( STALE_CACHE )); then
    echo "       Repairing migrated Tauri cache paths (one-time; keeping other compiled crates)…"
    cargo clean --manifest-path "$ROOT/src-tauri/Cargo.toml" \
      -p tauri -p tauri-build -p tauri-plugin-single-instance >/dev/null 2>&1 || true
    # Defensive cleanup for any path-sensitive Tauri build-script/fingerprint
    # remnants Cargo did not associate with the selected package IDs.
    for profile in debug release; do
      [[ -d "$CARGO_TARGET_DIR/$profile/build" ]] && \
        find "$CARGO_TARGET_DIR/$profile/build" -mindepth 1 -maxdepth 1 -type d \
          \( -name 'tauri-*' -o -name 'tauri-plugin-*' \) -exec rm -rf {} + 2>/dev/null || true
      [[ -d "$CARGO_TARGET_DIR/$profile/.fingerprint" ]] && \
        find "$CARGO_TARGET_DIR/$profile/.fingerprint" -mindepth 1 -maxdepth 1 -type d \
          \( -name 'tauri-*' -o -name 'tauri-plugin-*' \) -exec rm -rf {} + 2>/dev/null || true
    done
  fi
  mkdir -p "$(dirname "$CACHE_REPAIR_MARKER")"
  touch "$CACHE_REPAIR_MARKER"
fi

if ! cargo tauri --version >/dev/null 2>&1; then
  echo "[4/9] Installing Tauri CLI…"
  cargo install tauri-cli --version '^2' --locked
else
  echo "[4/9] Tauri CLI already installed."
fi

# Local Linux brain. Never pull a multi-GB model automatically; if the user
# already has qwen3.5:4b, create Fatir's concise system-operations profile.
if command -v ollama >/dev/null 2>&1; then
  if ! ollama list 2>/dev/null | awk 'NR>1 {print $1}' | grep -Eq '^fatir-local(:latest)?$'; then
    if ollama list 2>/dev/null | awk 'NR>1 {print $1}' | grep -Eq '^qwen3\.5:4b$'; then
      echo "       Creating local Fatir Linux profile from qwen3.5:4b…"
      ollama create fatir-local -f "$ROOT/ollama/Fatir-Modelfile" >/dev/null
    else
      echo "       Local model qwen3.5:4b is not installed; Auto will use cloud until a local model is available."
    fi
  else
    echo "       fatir-local model already available."
  fi
fi

# One-time data migration. Copy instead of moving so the old AH install remains recoverable.
if [[ -d "$HOME/.local/share/AH" && ! -e "$HOME/.local/share/Fatir" ]]; then
  echo "       Migrating AH data to Fatir…"
  cp -a "$HOME/.local/share/AH" "$HOME/.local/share/Fatir"
fi


# Ensure the tiny local router exists when qwen3.5:0.8b is already installed.
if command -v ollama >/dev/null 2>&1; then
  if ollama list 2>/dev/null | awk 'NR>1 {print $1}' | grep -Eq '^fatir-router(:latest)?$'; then
    echo "       Local router: fatir-router ready"
  elif ollama list 2>/dev/null | awk 'NR>1 {print $1}' | grep -Eq '^qwen3.5:0.8b$' && [ -f "$ROOT/ollama/Fatir-Router-Modelfile" ]; then
    echo "       Creating fast local router from existing qwen3.5:0.8b..."
    ollama create fatir-router -f "$ROOT/ollama/Fatir-Router-Modelfile" || true
  else
    echo "       Fast local router not installed (optional): ollama pull qwen3.5:0.8b"
  fi
fi

echo "[5/9] Validating Fatir v1.2.0 source…"
cd "$ROOT"
./scripts/validate.sh

echo "[6/9] Building Fatir v1.2.0…"
echo "       Shared Cargo cache: $CARGO_TARGET_DIR"
cargo metadata --manifest-path "$ROOT/src-tauri/Cargo.toml" --no-deps --format-version 1 >/dev/null
cargo tauri build --bundles deb

DEB="$(find "$CARGO_TARGET_DIR/release/bundle/deb" -maxdepth 1 -iname '*fatir*.deb' -type f 2>/dev/null | sort | tail -n1)"
if [[ -z "$DEB" || ! -f "$DEB" ]]; then
  DEB="$(find "$ROOT/src-tauri/target/release/bundle/deb" -maxdepth 1 -iname '*fatir*.deb' -type f 2>/dev/null | sort | tail -n1 || true)"
fi
if [[ -z "$DEB" || ! -f "$DEB" ]]; then
  echo "Fatir .deb was not produced." >&2
  exit 1
fi

echo "[7/9] Installing Fatir package…"
sudo apt install -y "$DEB"

# Remove the old AH package only after Fatir has installed successfully.
if dpkg-query -W -f='${Status}' ah-pa 2>/dev/null | grep -q 'install ok installed'; then
  sudo apt remove -y ah-pa >/dev/null 2>&1 || true
fi

echo "[8/9] Installing Cinnamon top-bar applet…"
OLD_APPLET="$HOME/.local/share/cinnamon/applets/ah@local"
APPLET_DIR="$HOME/.local/share/cinnamon/applets/fatir@local"
rm -rf "$OLD_APPLET" "$APPLET_DIR"
mkdir -p "$(dirname "$APPLET_DIR")"
cp -a "$ROOT/cinnamon-applet/fatir@local" "$APPLET_DIR"

python3 <<'PY'
import ast, subprocess
schema='org.cinnamon'
try:
    raw=subprocess.check_output(['gsettings','get',schema,'enabled-applets'],text=True).strip()
    items=ast.literal_eval(raw)
except Exception as e:
    print('Could not automatically add panel applet:', e)
    raise SystemExit(0)
items=[x for x in items if 'ah@local' not in x and 'fatir@local' not in x]
right=[]
for x in items:
    parts=x.split(':')
    if len(parts)>=5 and parts[0]=='panel1' and parts[1]=='right':
        try:right.append(int(parts[2]))
        except:pass
pos=max(right,default=0)+1
items.append(f'panel1:right:{pos}:fatir@local:990')
subprocess.check_call(['gsettings','set',schema,'enabled-applets',repr(items)])
PY

# Install Fatir Share into Nemo's right-click menu.
NEMO_ACTIONS="$HOME/.local/share/nemo/actions"
mkdir -p "$NEMO_ACTIONS"
cp "$ROOT/nemo-actions/fatir-share.nemo_action" "$NEMO_ACTIONS/fatir-share.nemo_action"

# Keep Bash history current so Fatir can learn committed terminal routines without keylogging.
if [[ -f "$HOME/.bashrc" ]]; then
python3 <<'PY'
from pathlib import Path
p=Path.home()/'.bashrc'
s=p.read_text()
for start,end in [('# >>> AH terminal learning >>>','# <<< AH terminal learning <<<'),('# >>> Fatir terminal learning >>>','# <<< Fatir terminal learning <<<')]:
    while start in s and end in s:
        a=s.index(start); b=s.index(end,a)+len(end)
        s=s[:a].rstrip()+"\n"+s[b:].lstrip("\n")
p.write_text(s)
PY
cat >> "$HOME/.bashrc" <<'FATIRBASH'

# >>> Fatir terminal learning >>>
shopt -s histappend 2>/dev/null || true
case ";${PROMPT_COMMAND:-};" in
  *";history -a;"*) ;;
  *) PROMPT_COMMAND="history -a${PROMPT_COMMAND:+; $PROMPT_COMMAND}" ;;
esac
# <<< Fatir terminal learning <<<
FATIRBASH
fi

mkdir -p "$HOME/.config/autostart"
rm -f "$HOME/.config/autostart/ah-pa.desktop"
cat > "$HOME/.config/autostart/fatir.desktop" <<EOF2
[Desktop Entry]
Type=Application
Name=Fatir Personal Assistant
Comment=Resident personal assistant
Exec=fatir --background
X-GNOME-Autostart-enabled=true
OnlyShowIn=X-Cinnamon;
NoDisplay=true
EOF2

echo "[9/9] Starting Fatir…"
pkill -x ah-pa 2>/dev/null || true
pkill -x fatir 2>/dev/null || true
nohup fatir --background >"$HOME/.cache/fatir.log" 2>&1 &
sleep 1
fatir >/dev/null 2>&1 &

echo
echo "=========================================="
echo " Fatir v1.2.0 is installed"
echo "=========================================="
echo
printf '%s\n' \
  "• Click Fatir in the Cinnamon top bar" \
  "• Auto mode uses Fast Paths first, then the tiny local router, then cloud only when needed" \
  "• V1 verifies GUI outcomes, classifies failures, remembers projects/terminal state, and records Teach 3.0 workflows" \
  "• Whole-Desktop Control uses semantic accessibility first, then keyboard/window and visual X11 fallbacks" \
  "• Persistent local schedules use systemd user timers; headless browser work remains explicit-only" \
  "• Fatir Share adds Nemo right-click sharing, WhatsApp, Gmail drafts and smart clipboard handoff" \
  "• ⌖ pauses/resumes computer control; ◇ pins Fatir above other apps when needed" \
  "• ▤ opens the V1 Command Center: Tasks, Jobs, Routines, Projects, Runtime, Cleanup, Alerts and Undo" \
  "• Existing AH memory/browser profile is copied forward on first install" \
  "• Future source builds reuse: $CARGO_TARGET_DIR"
