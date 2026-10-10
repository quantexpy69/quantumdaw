#!/usr/bin/env bash
# Notas de un release de GitHub a partir del commit de la etiqueta: secciones como títulos y viñetas,
# más la tabla de descargas (sin ella con --solo-notas).
# Uso: packaging/notas.sh v0.6.0 [--solo-notas]
set -euo pipefail
TAG=${1:?Uso: notas.sh vX.Y.Z [--solo-notas]}
VERSION=${TAG#v}
# Las líneas que no son viñetas ni continuación son títulos de sección.
git log -1 --format=%b "$TAG^{commit}" | sed -E 's/^([^ -].*)$/### \1/'
[ "${2:-}" = "--solo-notas" ] && exit 0
URL="https://github.com/quantexpy69/quantumdaw/releases/download/$TAG"
cat <<EOF

## Descargas

| Sistema | Archivo | Instalación |
|---|---|---|
| Windows 10 y 11 (64 bits) | [quantum-daw-windows-x64-setup.exe]($URL/quantum-daw-windows-x64-setup.exe) | ejecuta el instalador |
| Windows (portable) | [quantum-daw-windows-x64.zip]($URL/quantum-daw-windows-x64.zip) | descomprime y abre \`quantum-daw.exe\` |
| macOS 11 Big Sur o posterior (Intel y Apple Silicon) | [quantum-daw-macos-universal.dmg]($URL/quantum-daw-macos-universal.dmg) | arrastra Quantum DAW a Aplicaciones; la primera vez: clic derecho → Abrir |

### Linux

| Distribución | Archivo | Instalación |
|---|---|---|
| Debian, Ubuntu, Linux Mint, Pop!_OS, elementary… | [quantum-daw-amd64.deb]($URL/quantum-daw-amd64.deb) | \`sudo apt install ./quantum-daw-amd64.deb\` |
| Fedora, RHEL, CentOS Stream, Rocky Linux, AlmaLinux | [quantum-daw-x86_64.rpm]($URL/quantum-daw-x86_64.rpm) | \`sudo dnf install ./quantum-daw-x86_64.rpm\` |
| openSUSE | [quantum-daw-x86_64.rpm]($URL/quantum-daw-x86_64.rpm) | \`sudo zypper install ./quantum-daw-x86_64.rpm\` |
| Arch Linux, Manjaro, EndeavourOS | [PKGBUILD]($URL/PKGBUILD) | \`makepkg -si\` en la carpeta del PKGBUILD |
| Cualquier distribución (portable) | [quantum-daw-x86_64.AppImage]($URL/quantum-daw-x86_64.AppImage) | \`chmod +x quantum-daw-x86_64.AppImage && ./quantum-daw-x86_64.AppImage\` |
| Cualquier distribución (sin sudo) | [quantum-daw-linux-x86_64.tar.gz]($URL/quantum-daw-linux-x86_64.tar.gz) | descomprime y ejecuta \`./instalar.sh\` |

Linux: x86_64 con glibc 2.28 o posterior (Debian 10+, Ubuntu 20.04+, RHEL/Rocky/Alma 8+, Fedora, Arch).
Opcional: \`ffmpeg\` para importar video, \`7z\` y \`curl\` para los instrumentos descargables.
Verifica las descargas con [SHA256SUMS]($URL/SHA256SUMS): \`sha256sum -c SHA256SUMS\`.

**Versión $VERSION** · Licencia MPL-2.0 · [www.quantumdaw.com](https://www.quantumdaw.com)
EOF
