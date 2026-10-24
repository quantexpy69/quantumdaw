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

# Con la carpeta pro/ (repositorio privado) se incluyen los plugins QUANTUM Pro.
PRO=()
[ -f pro/mod.rs ] && PRO=(--features pro)
cargo build --release ${PRO[@]+"${PRO[@]}"}
BIN="$HOME/.local/bin/quantum-daw"
install -Dm755 target/release/quantum-daw "$BIN"
# Icono: SVG y PNG de cada tamaño (algunos paneles y docks prefieren PNG).
ICONOS="$HOME/.local/share/icons/hicolor"
install -Dm644 assets/quantumlogo.svg "$ICONOS/scalable/apps/quantum-daw.svg"
for t in 16:icon_16x16 32:icon_32x32 64:icon_32x32@2x 128:icon_128x128 256:icon_256x256 512:icon_512x512; do
  install -Dm644 "assets/macos/QuantumDAW.iconset/${t#*:}.png" "$ICONOS/${t%%:*}x${t%%:*}/apps/quantum-daw.png"
done
mkdir -p "$HOME/.local/share/applications"
sed "s|@BIN@|$BIN|" assets/quantum-daw.desktop > "$HOME/.local/share/applications/quantum-daw.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
# El escritorio guarda los iconos en caché: se le avisa que cambiaron (sin index.theme hace falta -t).
touch "$ICONOS"
gtk-update-icon-cache -q -f -t "$ICONOS" 2>/dev/null || true
echo "Listo: busca «QUANTUM» en el menú de aplicaciones. Vuelve a ejecutar ./install.sh tras cada cambio."
