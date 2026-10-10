#!/usr/bin/env bash
# Compila Quantum DAW para macOS 11 Big Sur o posterior como aplicación universal (Intel y Apple Silicon)
# y crea dist/quantum-daw-macos-universal.dmg. Se ejecuta en un Mac (o en GitHub Actions con macOS).
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/app/Cargo.toml | head -1)
export MACOSX_DEPLOYMENT_TARGET=11.0
rustup target add aarch64-apple-darwin x86_64-apple-darwin
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release --locked -p quantum-daw --target "$t"
done
mkdir -p dist
APP="dist/Quantum DAW.app"
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/quantum-daw" target/aarch64-apple-darwin/release/quantum-daw target/x86_64-apple-darwin/release/quantum-daw
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
iconutil -c icns assets/macos/QuantumDAW.iconset -o "$APP/Contents/Resources/QuantumDAW.icns"
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
# Firma ad hoc (sin cuenta de desarrollador): obligatoria para que arranque en Apple Silicon.
codesign --force --deep --sign - "$APP"
lipo -info "$APP/Contents/MacOS/quantum-daw"
"$APP/Contents/MacOS/quantum-daw" --version
# Imagen de disco con la app y un acceso a Aplicaciones para arrastrarla.
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Aplicaciones"
hdiutil create -volname "Quantum DAW $VERSION" -srcfolder "$STAGE" -ov -format UDZO dist/quantum-daw-macos-universal.dmg
rm -rf "$STAGE"
ls -lh dist
