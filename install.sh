#!/usr/bin/env bash
# Instala Quantum DAW para el usuario actual (sin sudo): binario, icono y lanzador en el menú de aplicaciones.
set -euo pipefail
cd "$(dirname "$0")"
source ./env.sh
cargo build --release
BIN="$HOME/.local/bin/quantum-daw"
install -Dm755 target/release/quantum-daw "$BIN"
install -Dm644 assets/quantumlogo.svg "$HOME/.local/share/icons/hicolor/scalable/apps/quantum-daw.svg"
mkdir -p "$HOME/.local/share/applications"
sed "s|@BIN@|$BIN|" assets/quantum-daw.desktop > "$HOME/.local/share/applications/quantum-daw.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "Listo: busca «Quantum DAW» en Actividades. Vuelve a ejecutar ./install.sh tras cada cambio."
