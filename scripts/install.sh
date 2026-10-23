#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked -j 1 --bin tend
python3 "$PWD/scripts/install-user.py"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
