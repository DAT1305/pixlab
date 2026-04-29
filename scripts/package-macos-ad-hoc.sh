#!/usr/bin/env bash
set -euo pipefail

APP_PATH="${APP_PATH:-src-tauri/target/release/bundle/macos/PixLab Desktop.app}"
DMG_DIR="${DMG_DIR:-src-tauri/target/release/bundle/dmg}"
VERSION="$(node -p "JSON.parse(require('fs').readFileSync('package.json', 'utf8')).version")"
ARCH="$(uname -m)"

case "$ARCH" in
  arm64) TAURI_ARCH="aarch64" ;;
  x86_64) TAURI_ARCH="x64" ;;
  *) TAURI_ARCH="$ARCH" ;;
esac

if [[ ! -d "$APP_PATH" ]]; then
  echo "App bundle not found: $APP_PATH" >&2
  exit 1
fi

codesign --force --deep --sign - "$APP_PATH"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"

WORK_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

mkdir -p "$DMG_DIR"
rm -f "$DMG_DIR"/*.dmg
cp -R "$APP_PATH" "$WORK_DIR/"
ln -s /Applications "$WORK_DIR/Applications"

hdiutil create \
  -volname "PixLab Desktop" \
  -srcfolder "$WORK_DIR" \
  -ov \
  -format UDZO \
  "$DMG_DIR/PixLab.Desktop_${VERSION}_${TAURI_ARCH}.dmg"

