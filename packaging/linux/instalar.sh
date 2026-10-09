#!/usr/bin/env bash
# Instala Quantum DAW desde el paquete .tar.gz en cualquier distribución Linux, sin compilar.
#   ./instalar.sh              → para tu usuario (~/.local), sin sudo
#   sudo ./instalar.sh --sistema → para todos los usuarios (/usr/local)
#   ./instalar.sh --desinstalar  → quita la instalación de tu usuario
set -euo pipefail
cd "$(dirname "$0")"

PREFIX="$HOME/.local"
case "${1:-}" in
  --sistema) PREFIX=/usr/local ;;
  --desinstalar)
    rm -f "$PREFIX/bin/quantum-daw" "$PREFIX/share/applications/quantum-daw.desktop" \
      "$PREFIX/share/icons/hicolor/scalable/apps/quantum-daw.svg" "$PREFIX/share/metainfo/com.quantumdaw.QuantumDAW.metainfo.xml"
    echo "Quantum DAW desinstalado (tus proyectos en ~/Documentos/Quantum DAW no se tocan)."
    exit 0 ;;
esac

install -Dm755 quantum-daw "$PREFIX/bin/quantum-daw"
install -Dm644 quantum-daw.svg "$PREFIX/share/icons/hicolor/scalable/apps/quantum-daw.svg"
install -Dm644 com.quantumdaw.QuantumDAW.metainfo.xml "$PREFIX/share/metainfo/com.quantumdaw.QuantumDAW.metainfo.xml"
sed "s|^Exec=quantum-daw|Exec=$PREFIX/bin/quantum-daw|" quantum-daw.desktop | install -Dm644 /dev/stdin "$PREFIX/share/applications/quantum-daw.desktop"
update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$PREFIX/share/icons/hicolor" 2>/dev/null || true

# Dependencias: ALSA es obligatoria; ffmpeg (video), 7z y curl (instrumentos) son opcionales.
. /etc/os-release 2>/dev/null || true
FAMILIA="${ID:-} ${ID_LIKE:-}"
faltan=()
ldconfig -p 2>/dev/null | grep -q 'libasound\.so\.2' || [ -e /usr/lib/libasound.so.2 ] || faltan+=(alsa)
for c in ffmpeg 7z curl; do command -v "$c" >/dev/null || faltan+=("$c"); done
if ((${#faltan[@]})); then
  echo "Faltan algunos componentes: ${faltan[*]}"
  case "$FAMILIA" in
    *debian* | *ubuntu*) echo "  sudo apt install libasound2 ffmpeg p7zip-full curl" ;;
    *fedora*) echo "  sudo dnf install alsa-lib ffmpeg-free p7zip curl" ;;
    *rhel* | *centos* | *rocky* | *alma*) echo "  sudo dnf install epel-release && sudo dnf install alsa-lib p7zip curl" \
      "  (ffmpeg: activa RPM Fusion → https://rpmfusion.org/Configuration)" ;;
    *arch* | *manjaro* | *endeavouros*) echo "  sudo pacman -S alsa-lib ffmpeg 7zip curl" ;;
    *suse*) echo "  sudo zypper install alsa ffmpeg p7zip curl" ;;
    *) echo "  Instálalos con el gestor de paquetes de tu distribución." ;;
  esac
  echo "  (ALSA es necesaria para el audio; el resto solo para video e instrumentos descargables)."
fi
echo "Listo: busca «Quantum DAW» en el menú de aplicaciones o ejecuta: $PREFIX/bin/quantum-daw"
