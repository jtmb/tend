#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked -j 1
pkexec /usr/bin/python3 "$PWD/scripts/install.py"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
python3 - <<'PY'
import ast,subprocess
key=['gsettings','get','org.gnome.shell','favorite-apps']
value=subprocess.check_output(key,text=True).strip()
apps=ast.literal_eval(value.removeprefix('@as '))
if 'io.github.jtmb.Tend.desktop' not in apps:
    apps.append('io.github.jtmb.Tend.desktop')
    subprocess.run(['gsettings','set','org.gnome.shell','favorite-apps',str(apps)],check=True)
PY
