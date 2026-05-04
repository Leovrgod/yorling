#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_NAME="Yorling"
APP_PATH="$ROOT_DIR/target/release/bundle/macos/$APP_NAME.app"
VERSION="$(node -p "require('./package.json').version")"
TAURI_VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
CARGO_WORKSPACE_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
ARCH="${YORLING_RELEASE_ARCH:-$(uname -m)}"
RELEASE_DIR="$ROOT_DIR/target/release/release-macos"
ZIP_PATH="$RELEASE_DIR/$APP_NAME-$VERSION-$ARCH.app.zip"
DMG_DIR="$ROOT_DIR/target/release/bundle/dmg"
DMG_PATH="$DMG_DIR/$APP_NAME-$VERSION-$ARCH.dmg"
STAGING_DIR="$RELEASE_DIR/dmg-staging"
UPDATER_ARCHIVE_PATH="$ROOT_DIR/target/release/bundle/macos/$APP_NAME.app.tar.gz"
UPDATER_SIGNATURE_PATH="$UPDATER_ARCHIVE_PATH.sig"
UPDATER_RELEASE_ARCHIVE_PATH="$RELEASE_DIR/$(basename "$UPDATER_ARCHIVE_PATH")"
UPDATER_RELEASE_SIGNATURE_PATH="$RELEASE_DIR/$(basename "$UPDATER_SIGNATURE_PATH")"
UPDATER_LATEST_JSON_PATH="$RELEASE_DIR/latest.json"
UPDATER_CONFIG_PATH="$RELEASE_DIR/tauri-updater.release.json"
UPDATER_ENABLED=1
DEFAULT_UPDATER_KEY_PATH="${YORLING_UPDATER_KEY_PATH:-$HOME/.tauri/yorling-updater.key}"
DEFAULT_UPDATER_PUBKEY_PATH="${YORLING_UPDATER_PUBKEY_PATH:-$DEFAULT_UPDATER_KEY_PATH.pub}"
GITHUB_REPOSITORY_SLUG="${YORLING_GITHUB_REPOSITORY:-Leovrgod/yorling}"
DEFAULT_UPDATER_ENDPOINT="https://github.com/$GITHUB_REPOSITORY_SLUG/releases/latest/download/latest.json"
DEFAULT_UPDATER_ARTIFACT_URL="https://github.com/$GITHUB_REPOSITORY_SLUG/releases/latest/download/$APP_NAME.app.tar.gz"

export VERSION
export UPDATER_CONFIG_PATH
export UPDATER_RELEASE_SIGNATURE_PATH
export UPDATER_LATEST_JSON_PATH

if [[ "${YORLING_SKIP_UPDATER:-}" == "1" ]]; then
  UPDATER_ENABLED=0
fi

if [[ "$VERSION" != "$TAURI_VERSION" || "$VERSION" != "$CARGO_WORKSPACE_VERSION" ]]; then
  echo "Version mismatch: package.json=$VERSION, src-tauri/tauri.conf.json=$TAURI_VERSION, Cargo.toml=$CARGO_WORKSPACE_VERSION." >&2
  exit 1
fi

case "$ARCH" in
  arm64|aarch64)
    DEFAULT_UPDATER_TARGET="darwin-aarch64"
    ;;
  x86_64|amd64)
    DEFAULT_UPDATER_TARGET="darwin-x86_64"
    ;;
  *)
    DEFAULT_UPDATER_TARGET="darwin-$ARCH"
    ;;
esac

export YORLING_UPDATER_TARGET="${YORLING_UPDATER_TARGET:-$DEFAULT_UPDATER_TARGET}"

SIGNING_IDENTITY="${YORLING_CODESIGN_IDENTITY:-${APPLE_SIGNING_IDENTITY:-}}"
if [[ -z "$SIGNING_IDENTITY" ]]; then
  echo "Missing signing identity. Set YORLING_CODESIGN_IDENTITY or APPLE_SIGNING_IDENTITY." >&2
  exit 1
fi

notary_args=()
if [[ -n "${APPLE_API_KEY_PATH:-}" && -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" ]]; then
  if [[ ! -f "$APPLE_API_KEY_PATH" ]]; then
    echo "APPLE_API_KEY_PATH does not point to a readable file." >&2
    exit 1
  fi
  notary_args=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER")
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
  notary_args=(--apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID")
else
  echo "Missing notarization credentials. Set APPLE_API_KEY_PATH/APPLE_API_KEY/APPLE_API_ISSUER or APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID." >&2
  exit 1
fi

mkdir -p "$RELEASE_DIR"

updater_build_args=()
if [[ "$UPDATER_ENABLED" == "1" ]]; then
  if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -n "${YORLING_UPDATER_PRIVATE_KEY:-}" ]]; then
    export TAURI_SIGNING_PRIVATE_KEY="$YORLING_UPDATER_PRIVATE_KEY"
  fi
  if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -f "$DEFAULT_UPDATER_KEY_PATH" ]]; then
    export TAURI_SIGNING_PRIVATE_KEY="$DEFAULT_UPDATER_KEY_PATH"
  fi
  if [[ -z "${YORLING_UPDATER_PUBKEY:-}" && -f "$DEFAULT_UPDATER_PUBKEY_PATH" ]]; then
    export YORLING_UPDATER_PUBKEY="$(cat "$DEFAULT_UPDATER_PUBKEY_PATH")"
  fi
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
  export YORLING_UPDATER_ENDPOINT="${YORLING_UPDATER_ENDPOINT:-$DEFAULT_UPDATER_ENDPOINT}"
  export YORLING_UPDATER_ARTIFACT_URL="${YORLING_UPDATER_ARTIFACT_URL:-$DEFAULT_UPDATER_ARTIFACT_URL}"

  if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
    echo "Missing updater signing key. Set TAURI_SIGNING_PRIVATE_KEY, YORLING_UPDATER_PRIVATE_KEY, or YORLING_UPDATER_KEY_PATH, or set YORLING_SKIP_UPDATER=1." >&2
    exit 1
  fi

  if [[ -z "${YORLING_UPDATER_PUBKEY:-}" || -z "${YORLING_UPDATER_ENDPOINT:-}" ]]; then
    echo "Missing updater runtime config. Set YORLING_UPDATER_PUBKEY/YORLING_UPDATER_PUBKEY_PATH and YORLING_UPDATER_ENDPOINT, or set YORLING_SKIP_UPDATER=1." >&2
    exit 1
  fi

  node <<'NODE'
const fs = require('node:fs');

const path = process.env.UPDATER_CONFIG_PATH;
const pubkey = process.env.YORLING_UPDATER_PUBKEY;
const endpoint = process.env.YORLING_UPDATER_ENDPOINT;

fs.writeFileSync(
  path,
  JSON.stringify({
    bundle: {
      createUpdaterArtifacts: true
    },
    plugins: {
      updater: {
        pubkey,
        endpoints: [endpoint]
      }
    }
  }, null, 2)
);
NODE
  updater_build_args=(--config "$UPDATER_CONFIG_PATH")
fi

step() {
  printf '\n==> %s\n' "$1"
}

notarize_artifact() {
  local artifact_path="$1"
  local artifact_name="$2"
  local submit_output submission_id status info_output attempt
  local poll_seconds="${YORLING_NOTARY_POLL_SECONDS:-10}"
  local max_attempts="${YORLING_NOTARY_MAX_ATTEMPTS:-120}"

  step "Submitting $artifact_name for notarization"
  submit_output="$(xcrun notarytool submit "$artifact_path" "${notary_args[@]}" --output-format json 2>&1)"
  printf '%s\n' "$submit_output"
  submission_id="$(
    printf '%s\n' "$submit_output" \
      | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
      | head -n 1
  )"

  if [[ -z "$submission_id" ]]; then
    echo "Could not find a notarization submission id for $artifact_path." >&2
    exit 1
  fi

  for ((attempt = 1; attempt <= max_attempts; attempt++)); do
    if info_output="$(xcrun notarytool info "$submission_id" "${notary_args[@]}" --output-format json 2>&1)"; then
      status="$(
        printf '%s\n' "$info_output" \
          | sed -n 's/.*"status"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
          | head -n 1
      )"
      case "$status" in
        Accepted)
          echo "$artifact_name notarization accepted: $submission_id"
          return
          ;;
        Invalid|Rejected)
          echo "$artifact_name notarization failed: $submission_id" >&2
          xcrun notarytool log "$submission_id" "${notary_args[@]}" || true
          exit 1
          ;;
        *)
          echo "$artifact_name notarization status: ${status:-unknown} ($attempt/$max_attempts)"
          ;;
      esac
    else
      echo "$artifact_name notarization status check failed; retrying ($attempt/$max_attempts)." >&2
      printf '%s\n' "$info_output" >&2
    fi
    sleep "$poll_seconds"
  done

  echo "$artifact_name notarization did not finish in time: $submission_id" >&2
  exit 1
}

step "Building signed app bundle"
# Tauri notarization can leave the release in an opaque half-state if stapling or
# follow-up bundling stalls. Build the signed .app first, then notarize explicitly.
env \
  -u APPLE_API_ISSUER \
  -u APPLE_API_KEY \
  -u APPLE_API_KEY_PATH \
  -u APPLE_ID \
  -u APPLE_PASSWORD \
  -u APPLE_TEAM_ID \
  pnpm tauri build --bundles app "${updater_build_args[@]}"

if [[ ! -d "$APP_PATH" ]]; then
  echo "Expected app bundle was not produced: $APP_PATH" >&2
  exit 1
fi

if [[ "$UPDATER_ENABLED" == "1" ]]; then
  if [[ ! -f "$UPDATER_ARCHIVE_PATH" || ! -f "$UPDATER_SIGNATURE_PATH" ]]; then
    echo "Expected updater artifacts were not produced: $UPDATER_ARCHIVE_PATH and $UPDATER_SIGNATURE_PATH" >&2
    exit 1
  fi
  cp "$UPDATER_ARCHIVE_PATH" "$UPDATER_RELEASE_ARCHIVE_PATH"
  cp "$UPDATER_SIGNATURE_PATH" "$UPDATER_RELEASE_SIGNATURE_PATH"
fi

step "Verifying app signature"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"
codesign -dvvv "$APP_PATH" 2>&1 | sed -n '/Authority=/p;/TeamIdentifier=/p;/flags=/p'

rm -f "$ZIP_PATH"
ditto -c -k --keepParent "$APP_PATH" "$ZIP_PATH"
notarize_artifact "$ZIP_PATH" "app"

step "Stapling app ticket"
xcrun stapler staple "$APP_PATH"
xcrun stapler validate "$APP_PATH"
spctl -a -vv --type execute "$APP_PATH"

step "Creating DMG"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR" "$DMG_DIR"
ditto "$APP_PATH" "$STAGING_DIR/$APP_NAME.app"
ln -s /Applications "$STAGING_DIR/Applications"
rm -f "$DMG_PATH"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGING_DIR" -ov -format UDZO "$DMG_PATH"

step "Signing DMG"
codesign --force --sign "$SIGNING_IDENTITY" --timestamp "$DMG_PATH"
codesign --verify --verbose=2 "$DMG_PATH"

notarize_artifact "$DMG_PATH" "DMG"

step "Stapling DMG ticket"
xcrun stapler staple "$DMG_PATH"
xcrun stapler validate "$DMG_PATH"
spctl -a -vv -t open --context context:primary-signature "$DMG_PATH"

if [[ "$UPDATER_ENABLED" == "1" && -n "${YORLING_UPDATER_ARTIFACT_URL:-}" ]]; then
  step "Writing updater latest.json"
  node <<'NODE'
const fs = require('node:fs');

const version = process.env.VERSION;
const target = process.env.YORLING_UPDATER_TARGET;
const url = process.env.YORLING_UPDATER_ARTIFACT_URL;
const signaturePath = process.env.UPDATER_RELEASE_SIGNATURE_PATH;
const latestJsonPath = process.env.UPDATER_LATEST_JSON_PATH;

const signature = fs.readFileSync(signaturePath, 'utf8').trim();
fs.writeFileSync(
  latestJsonPath,
  JSON.stringify({
    version,
    notes: process.env.YORLING_UPDATER_NOTES || '',
    pub_date: new Date().toISOString(),
    platforms: {
      [target]: {
        signature,
        url
      }
    }
  }, null, 2)
);
NODE
fi

step "Release artifact ready"
echo "$DMG_PATH"
if [[ "$UPDATER_ENABLED" == "1" ]]; then
  echo "$UPDATER_RELEASE_ARCHIVE_PATH"
  echo "$UPDATER_RELEASE_SIGNATURE_PATH"
  if [[ -f "$UPDATER_LATEST_JSON_PATH" ]]; then
    echo "$UPDATER_LATEST_JSON_PATH"
  else
    echo "Set YORLING_UPDATER_ARTIFACT_URL to generate $UPDATER_LATEST_JSON_PATH"
  fi
fi
