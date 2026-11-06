#!/bin/bash
set -euo pipefail

BIN_DIR="${HOME}/.local/bin"
mkdir -p "${BIN_DIR}"

echo "🔨 Baue midikontrol-rust in Release-Konfiguration..."
cargo build --release

echo "📦 Installiere Binary nach ${BIN_DIR}/..."
install -m 755 target/release/midikontrol-rust "${BIN_DIR}/midikontrol-rust"
ln -sf "${BIN_DIR}/midikontrol-rust" "${BIN_DIR}/midicontrol-rust"

echo "🔄 Starte systemd-Service neu..."
if systemctl --user list-unit-files | grep -q "midicontrol.service"; then
    systemctl --user restart midicontrol.service
    echo "✅ midicontrol.service erfolgreich neu gestartet!"
    systemctl --user status midicontrol.service --no-pager
else
    echo "⚠️  Hinweis: midicontrol.service wurde in systemctl --user nicht gefunden."
fi
