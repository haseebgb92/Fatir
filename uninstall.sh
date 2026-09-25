#!/usr/bin/env bash
set -euo pipefail

pkill -x fatir 2>/dev/null || true
pkill -x ah-pa 2>/dev/null || true
rm -f "$HOME/.local/share/Fatir/pointer.sock" 2>/dev/null || true
rm -f "$HOME/.config/autostart/fatir.desktop"
rm -f "$HOME/.local/share/nemo/actions/fatir-share.nemo_action" "$HOME/.config/autostart/ah-pa.desktop"
rm -rf "$HOME/.local/share/cinnamon/applets/fatir@local" "$HOME/.local/share/cinnamon/applets/ah@local"

if [[ -f "$HOME/.bashrc" ]]; then
  python3 - <<'PY2'
from pathlib import Path
p=Path.home()/'.bashrc'
s=p.read_text()
for start,end in [('# >>> Fatir terminal learning >>>','# <<< Fatir terminal learning <<<'),('# >>> AH terminal learning >>>','# <<< AH terminal learning <<<')]:
    while start in s and end in s:
        a=s.index(start); b=s.index(end,a)+len(end)
        s=s[:a].rstrip()+"\n"+s[b:].lstrip("\n")
p.write_text(s)
PY2
fi

python3 <<'PY'
import ast, subprocess
try:
    raw=subprocess.check_output(['gsettings','get','org.cinnamon','enabled-applets'],text=True).strip()
    items=ast.literal_eval(raw)
    items=[x for x in items if 'fatir@local' not in x and 'ah@local' not in x]
    subprocess.check_call(['gsettings','set','org.cinnamon','enabled-applets',repr(items)])
except Exception as e:
    print('Panel cleanup:', e)
PY

for pkg in fatir ah-pa; do
  if dpkg-query -W -f='${Status}' "$pkg" 2>/dev/null | grep -q 'install ok installed'; then
    sudo apt remove -y "$pkg"
  fi
done

echo "Fatir application removed."
echo "Your local Fatir data in ~/.local/share/Fatir is intentionally preserved."
