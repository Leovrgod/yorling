#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAURI_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR="$TAURI_DIR/target/finder-sync"
APPEX_DIR="$OUT_DIR/YorlingFinderSync.appex"
CONTENTS_DIR="$APPEX_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
EXECUTABLE="$MACOS_DIR/YorlingFinderSync"
INFO_PLIST="$CONTENTS_DIR/Info.plist"
SDK_PATH="$(xcrun --sdk macosx --show-sdk-path)"
MIN_MACOS_VERSION="${YORLING_FINDER_SYNC_MIN_MACOS:-13.0}"
IDENTITY="${YORLING_CODESIGN_IDENTITY:-${APPLE_SIGNING_IDENTITY:-${CODESIGN_IDENTITY:--}}}"
TIMESTAMP_ARG="${YORLING_CODESIGN_TIMESTAMP:---timestamp}"

rm -rf "$APPEX_DIR"
mkdir -p "$MACOS_DIR"
cp "$SCRIPT_DIR/Info.plist" "$INFO_PLIST"

if command -v plutil >/dev/null 2>&1; then
  plutil -replace LSMinimumSystemVersion -string "$MIN_MACOS_VERSION" "$INFO_PLIST"
fi

compile_arch() {
  local arch="$1"
  local output="$2"
  local target="${arch}-apple-macos${MIN_MACOS_VERSION}"

  swiftc \
    -target "$target" \
    -sdk "$SDK_PATH" \
    -module-name YorlingFinderSync \
    -parse-as-library \
    -Osize \
    -framework AppKit \
    -framework FinderSync \
    -framework Foundation \
    -Xlinker -e \
    -Xlinker _NSExtensionMain \
    "$SCRIPT_DIR/Sources/YorlingFinderSync.swift" \
    -o "$output"
}

case "${TAURI_ENV_ARCH:-$(uname -m)}" in
  aarch64|arm64)
    compile_arch arm64 "$EXECUTABLE"
    ;;
  x86_64)
    compile_arch x86_64 "$EXECUTABLE"
    ;;
  universal|universal-apple-darwin)
    TMP_DIR="$(mktemp -d)"
    compile_arch arm64 "$TMP_DIR/YorlingFinderSync-arm64"
    compile_arch x86_64 "$TMP_DIR/YorlingFinderSync-x86_64"
    lipo -create "$TMP_DIR/YorlingFinderSync-arm64" "$TMP_DIR/YorlingFinderSync-x86_64" -output "$EXECUTABLE"
    rm -rf "$TMP_DIR"
    ;;
  *)
    compile_arch "$(uname -m)" "$EXECUTABLE"
    ;;
esac

chmod 755 "$EXECUTABLE"

if [[ "$IDENTITY" == "-" ]]; then
  codesign --force --sign - --timestamp=none --entitlements "$SCRIPT_DIR/YorlingFinderSync.entitlements" "$APPEX_DIR"
else
  codesign --force --sign "$IDENTITY" "$TIMESTAMP_ARG" --options runtime --entitlements "$SCRIPT_DIR/YorlingFinderSync.entitlements" "$APPEX_DIR"
fi

codesign --verify --strict "$APPEX_DIR"
echo "Built $APPEX_DIR"
