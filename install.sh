#!/usr/bin/env bash
# Compila e instala Quantum DAW desde el código para el usuario actual (sin sudo), en cualquier
# distribución: binario, icono y lanzador en el menú de aplicaciones.
set -euo pipefail
cd "$(dirname "$0")"
source ./env.sh

# Rust: el del proyecto (.toolchain); si no está, se instala ahí mismo con rustup.
if ! command -v cargo >/dev/null; then
  echo "Instalando Rust en .toolchain/ (solo para este proyecto)…"
  curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --no-modify-path
fi

# Cabeceras de ALSA (necesarias para compilar el audio).
if ! pkg-config --exists alsa 2>/dev/null && [ ! -f .toolchain/sysroot/usr/lib64/pkgconfig/alsa.pc ]; then
  . /etc/os-release 2>/dev/null || true
  echo "Faltan las herramientas de compilación o las cabeceras de ALSA. Instálalas con:"
  case "${ID:-} ${ID_LIKE:-}" in
    *debian* | *ubuntu*) echo "  sudo apt install build-essential pkg-config libasound2-dev" ;;
    *fedora* | *rhel* | *centos* | *rocky* | *alma*) echo "  sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel" ;;
    *arch* | *manjaro* | *endeavouros*) echo "  sudo pacman -S --needed base-devel alsa-lib" ;;
    *suse*) echo "  sudo zypper install gcc pkg-config alsa-devel" ;;
    *) echo "  el compilador de C, pkg-config y el paquete de desarrollo de ALSA de tu distribución" ;;
  esac
  exit 1
fi

cargo build --release
BIN="$HOME/.local/bin/quantum-daw"
install -Dm755 target/release/quantum-daw "$BIN"
install -Dm644 assets/quantumlogo.svg "$HOME/.local/share/icons/hicolor/scalable/apps/quantum-daw.svg"
mkdir -p "$HOME/.local/share/applications"
sed "s|@BIN@|$BIN|" assets/quantum-daw.desktop > "$HOME/.local/share/applications/quantum-daw.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "Listo: busca «Quantum DAW» en el menú de aplicaciones. Vuelve a ejecutar ./install.sh tras cada cambio."
