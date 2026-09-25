#!/usr/bin/env bash
set -euo pipefail

PACKAGES=(
  at-spi2-core
  python3-pyatspi
  python3-gi
  python3-cairo
  gir1.2-atspi-2.0
  gir1.2-gtk-3.0
  libatk-wrapper-java
  libatk-wrapper-java-jni
  xdotool
  wmctrl
  xclip
  x11-utils
  xauth
  gnome-screenshot
  scrot
  xdg-utils
  desktop-file-utils
  libglib2.0-bin
  libgtk-3-bin
  dbus-x11
  libnotify-bin
  policykit-1
  gvfs-backends
  shared-mime-info
  gnome-keyring
  flatpak
  poppler-utils
  unzip
  tar
  procps
  psmisc
)

if [[ "${FATIR_SKIP_APT:-0}" != "1" ]]; then
  echo "[1/6] Installing Fatir local-runtime dependencies…"
  sudo apt-get update
  sudo apt-get install -y "${PACKAGES[@]}"
else
  echo "[1/6] Local-runtime dependencies were already handled by the Fatir installer."
fi

echo "[2/6] Enabling toolkit accessibility for this desktop session…"
if gsettings writable org.gnome.desktop.interface toolkit-accessibility >/dev/null 2>&1; then
  gsettings set org.gnome.desktop.interface toolkit-accessibility true || true
fi

echo "[3/6] Configuring existing JetBrains-family IDE profiles…"
shopt -s nullglob
changed=0
profiles=("$HOME"/.config/Google/AndroidStudio* "$HOME"/.config/JetBrains/*)
for dir in "${profiles[@]}"; do
  [[ -d "$dir" ]] || continue
  base="$(basename "$dir")"
  case "${base,,}" in
    androidstudio*|intellijidea*|pycharm*|webstorm*|clion*|phpstorm*|goland*|datagrip*|rubymine*|rider*|rustrover*) ;;
    *) continue ;;
  esac

  prop="$dir/idea.properties"
  if ! grep -qxF 'ide.support.screenreaders.enabled=true' "$prop" 2>/dev/null; then
    [[ ! -f "$prop" ]] || cp -a "$prop" "$prop.fatir-backup-$(date +%Y%m%d-%H%M%S)"
    printf '\nide.support.screenreaders.enabled=true\n' >> "$prop"
    changed=1
  fi

  vm=""
  while IFS= read -r existing; do vm="$existing"; break; done < <(find "$dir" -maxdepth 1 -type f \( -name '*64.vmoptions' -o -name '*.vmoptions' \) | sort)
  if [[ -z "$vm" ]]; then
    case "${base,,}" in
      androidstudio*) vm="$dir/studio64.vmoptions" ;;
      pycharm*) vm="$dir/pycharm64.vmoptions" ;;
      webstorm*) vm="$dir/webstorm64.vmoptions" ;;
      clion*) vm="$dir/clion64.vmoptions" ;;
      phpstorm*) vm="$dir/phpstorm64.vmoptions" ;;
      goland*) vm="$dir/goland64.vmoptions" ;;
      datagrip*) vm="$dir/datagrip64.vmoptions" ;;
      rubymine*) vm="$dir/rubymine64.vmoptions" ;;
      rider*) vm="$dir/rider64.vmoptions" ;;
      rustrover*) vm="$dir/rustrover64.vmoptions" ;;
      *) vm="$dir/idea64.vmoptions" ;;
    esac
  fi
  if ! grep -qxF -- '-Djavax.accessibility.assistive_technologies=org.GNOME.Accessibility.AtkWrapper' "$vm" 2>/dev/null; then
    [[ ! -f "$vm" ]] || cp -a "$vm" "$vm.fatir-backup-$(date +%Y%m%d-%H%M%S)"
    printf '\n-Djavax.accessibility.assistive_technologies=org.GNOME.Accessibility.AtkWrapper\n' >> "$vm"
    changed=1
  fi
done

echo "[4/6] Testing semantic AT-SPI access…"
python3 - <<'PY'
import pyatspi
root = pyatspi.Registry.getDesktop(0)
print(f"AT-SPI OK: {root.childCount} top-level accessibility applications visible")
PY

echo "[5/6] Testing window/input/visual fallbacks…"
printf 'Session: %s\n' "${XDG_SESSION_TYPE:-unknown}"
for cmd in wmctrl xdotool xclip xprop gio xdg-open gsettings gdbus gtk-launch update-desktop-database notify-send pkexec flatpak dpkg-query dpkg-deb pdfinfo pdftotext pdftoppm unzip tar systemctl systemd-run ps df du sha256sum git; do
  if command -v "$cmd" >/dev/null 2>&1; then
    printf '  %-24s OK\n' "$cmd"
  else
    printf '  %-24s MISSING\n' "$cmd"
  fi
done
if command -v gnome-screenshot >/dev/null 2>&1 || command -v scrot >/dev/null 2>&1; then
  echo "  screenshot capture       OK"
else
  echo "  screenshot capture       MISSING"
fi
if [[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "  D-Bus session            OK"
else
  echo "  D-Bus session            WARNING: no DBUS_SESSION_BUS_ADDRESS in this shell"
fi

echo "[6/6] Local runtime summary…"
python3 - <<'PY2'
import os, shutil
required = [
    'wmctrl','xdotool','xclip','xprop','gio','xdg-open','gsettings','gdbus','gtk-launch',
    'update-desktop-database','notify-send','pkexec','flatpak','dpkg-query','dpkg-deb',
    'pdfinfo','pdftotext','pdftoppm','unzip','tar','systemctl','systemd-run',
    'ps','df','du','sha256sum','git'
]
missing=[x for x in required if shutil.which(x) is None]
screenshot = bool(shutil.which('gnome-screenshot') or shutil.which('scrot'))
print('Core commands:', 'OK' if not missing else 'MISSING: '+', '.join(missing))
print('Screenshot backend:', 'OK' if screenshot else 'MISSING')
print('Session bus:', 'OK' if os.environ.get('DBUS_SESSION_BUS_ADDRESS') else 'WARNING')
if missing or not screenshot:
    raise SystemExit(2)
PY2

echo
if (( changed )); then
  echo "JetBrains accessibility settings were updated. Close and reopen any running JetBrains IDEs."
fi
echo "Fatir local runtime is ready: AT-SPI -> keyboard/window -> visual X11 fallback, with clipboard, launchers, PolicyKit, notifications, keyring, file/PDF/archive helpers and background-job support."
echo "If AT-SPI still sees no applications, log out and back in once to refresh the accessibility session bus."
