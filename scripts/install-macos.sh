#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "Bu script yalnizca macOS icin." >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Kodmoz Code"
BUNDLE_ID="com.kodmoz.code"
VERSION="$(awk -F'"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml")"
ICON_SRC="$ROOT/assets/icon-1024.png"

if [ -z "$VERSION" ]; then
  VERSION="0.1.0"
fi
if [ ! -f "$ICON_SRC" ]; then
  echo "Ikon bulunamadi: $ICON_SRC" >&2
  exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "==> Release derlemesi"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo "==> Ikon ve .app paketi hazirlaniyor"
ICONSET="$TMP/AppIcon.iconset"
mkdir -p "$ICONSET"
for spec in \
  "16 icon_16x16" \
  "32 icon_16x16@2x" \
  "32 icon_32x32" \
  "64 icon_32x32@2x" \
  "128 icon_128x128" \
  "256 icon_128x128@2x" \
  "256 icon_256x256" \
  "512 icon_256x256@2x" \
  "512 icon_512x512" \
  "1024 icon_512x512@2x"; do
  size="${spec%% *}"
  name="${spec##* }"
  sips -z "$size" "$size" "$ICON_SRC" --out "$ICONSET/$name.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$TMP/AppIcon.icns"

APP_TMP="$TMP/$APP_NAME.app"
mkdir -p "$APP_TMP/Contents/MacOS" "$APP_TMP/Contents/Resources"
cp "$ROOT/target/release/kodmoz-code" "$APP_TMP/Contents/MacOS/kodmoz-code"
chmod 755 "$APP_TMP/Contents/MacOS/kodmoz-code"
cp "$TMP/AppIcon.icns" "$APP_TMP/Contents/Resources/AppIcon.icns"
printf 'APPL????' > "$APP_TMP/Contents/PkgInfo"

cat > "$APP_TMP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleExecutable</key>
  <string>kodmoz-code</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.developer-tools</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSHumanReadableCopyright</key>
  <string>MIT © 2026 Kodmoz Code contributors</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
  <key>NSSupportsAutomaticGraphicsSwitching</key>
  <true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP_TMP" >/dev/null 2>&1 || true

INSTALL_DIR="/Applications"
if [ ! -w "$INSTALL_DIR" ]; then
  INSTALL_DIR="$HOME/Applications"
fi
APP="$INSTALL_DIR/$APP_NAME.app"
mkdir -p "$INSTALL_DIR"

if [ -d "$APP" ]; then
  EXISTING_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist" 2>/dev/null || true)"
  if [ -n "$EXISTING_ID" ] && [ "$EXISTING_ID" != "$BUNDLE_ID" ]; then
    echo "Hata: $APP baska bir uygulamaya ait ($EXISTING_ID). Uzerine yazilmiyor." >&2
    exit 1
  fi
  rm -rf "$APP"
fi

ditto "$APP_TMP" "$APP"
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
if [ -x "$LSREGISTER" ]; then
  "$LSREGISTER" -f "$APP" >/dev/null 2>&1 || true
fi
mdimport "$APP" >/dev/null 2>&1 || true

echo ""
echo "Kuruldu: $APP"
echo "Baslatmak icin: open -a \"$APP_NAME\""
echo "Spotlight/Launchpad'de 'Kodmoz Code' olarak gorunur."
