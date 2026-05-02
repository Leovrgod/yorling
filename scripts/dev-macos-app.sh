#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PATH="$ROOT_DIR/target/debug/bundle/macos/Yorling.app"
APPEX_PATH="$APP_PATH/Contents/PlugIns/YorlingFinderSync.appex"
SIDECAR_PATH="$APP_PATH/Contents/MacOS/yorling-bridge"
FINDER_SYNC_ENTITLEMENTS="$ROOT_DIR/src-tauri/macos/YorlingFinderSync/YorlingFinderSync.entitlements"
MAIN_BUNDLE_ID="com.yorling.app"
APP_PROCESS_NAME="yorling"
FINDER_SYNC_BUNDLE_ID="com.yorling.app.FinderSync"
FINDER_SYNC_PROCESS_NAME="YorlingFinderSync"

cd "$ROOT_DIR"

resolve_codesign_identity() {
  if [[ -n "${YORLING_CODESIGN_IDENTITY:-}" ]]; then
    echo "$YORLING_CODESIGN_IDENTITY"
    return
  fi

  if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    echo "$APPLE_SIGNING_IDENTITY"
    return
  fi

  if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
    echo "$CODESIGN_IDENTITY"
    return
  fi

  local development_identity
  development_identity="$(
    security find-identity -v -p codesigning 2>/dev/null \
      | awk -F '"' '/Apple Development:/ { print $2; exit }'
  )"

  if [[ -n "$development_identity" ]]; then
    echo "$development_identity"
  else
    echo "-"
  fi
}

IDENTITY="$(resolve_codesign_identity)"
export YORLING_CODESIGN_IDENTITY="$IDENTITY"
export YORLING_CODESIGN_TIMESTAMP="${YORLING_CODESIGN_TIMESTAMP:---timestamp=none}"

quit_running_app() {
  if ! pgrep -x "$APP_PROCESS_NAME" >/dev/null 2>&1; then
    return
  fi

  echo "Quitting running Yorling before opening the rebuilt app..."
  /usr/bin/osascript -e "tell application id \"$MAIN_BUNDLE_ID\" to quit" >/dev/null 2>&1 || true

  for _ in {1..40}; do
    if ! pgrep -x "$APP_PROCESS_NAME" >/dev/null 2>&1; then
      return
    fi
    sleep 0.25
  done

  echo "Yorling did not quit in time; sending SIGTERM to $APP_PROCESS_NAME." >&2
  /usr/bin/pkill -TERM -x "$APP_PROCESS_NAME" || true

  for _ in {1..20}; do
    if ! pgrep -x "$APP_PROCESS_NAME" >/dev/null 2>&1; then
      return
    fi
    sleep 0.25
  done
}

pnpm tauri build --debug --bundles app

if [[ ! -d "$APP_PATH" ]]; then
  echo "Yorling.app was not produced at $APP_PATH" >&2
  exit 1
fi

if [[ ! -d "$APPEX_PATH" ]]; then
  echo "YorlingFinderSync.appex was not embedded at $APPEX_PATH" >&2
  exit 1
fi

codesign_args=(--force --sign "$IDENTITY" --timestamp=none)
if [[ "$IDENTITY" != "-" ]]; then
  codesign_args+=(--options runtime)
fi

codesign "${codesign_args[@]}" --entitlements "$FINDER_SYNC_ENTITLEMENTS" "$APPEX_PATH"
if [[ -f "$SIDECAR_PATH" ]]; then
  codesign "${codesign_args[@]}" "$SIDECAR_PATH"
fi
codesign "${codesign_args[@]}" "$APP_PATH"

codesign --verify --strict --verbose=2 "$APPEX_PATH"
codesign --verify --strict --verbose=2 "$APP_PATH"

/usr/bin/pluginkit -r "$APPEX_PATH" || true
/usr/bin/pluginkit -a "$APPEX_PATH" || true
/usr/bin/pluginkit -e use -p com.apple.FinderSync -i "$FINDER_SYNC_BUNDLE_ID"
/usr/bin/pkill -x "$FINDER_SYNC_PROCESS_NAME" || true
sleep 1

opened="yes"
if [[ "${YORLING_SKIP_OPEN:-0}" != "1" ]]; then
  if [[ "${YORLING_RESTART_RUNNING:-1}" != "0" ]]; then
    quit_running_app
  fi
  /usr/bin/open "$APP_PATH"
else
  opened="no"
fi

echo
echo "Codesigned with: $IDENTITY"
if [[ "$opened" == "yes" ]]; then
  echo "Opened $APP_PATH"
else
  echo "Prepared $APP_PATH"
fi
echo "Finder Sync status:"
finder_sync_status=""
for _ in {1..10}; do
  finder_sync_status="$(/usr/bin/pluginkit -m -A -D -p com.apple.FinderSync -i "$FINDER_SYNC_BUNDLE_ID" || true)"
  if [[ "$finder_sync_status" == *"$FINDER_SYNC_BUNDLE_ID"* ]]; then
    echo "$finder_sync_status"
    break
  fi
  sleep 0.2
done

if [[ "$finder_sync_status" != *"$FINDER_SYNC_BUNDLE_ID"* ]]; then
  echo "$finder_sync_status"
fi
