#!/usr/bin/env bash
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then
  echo "This diagnostic requires macOS." >&2
  exit 1
fi
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="$ROOT_DIR/target/diagnostics"
mkdir -p "$OUTPUT_DIR"
xcrun clang -Wall -Wextra -Werror -O2 "$ROOT_DIR/scripts/diagnose-mouse-macos.c" \
  -framework ApplicationServices -o "$OUTPUT_DIR/diagnose-mouse-macos"
"$OUTPUT_DIR/diagnose-mouse-macos" "${@}" | tee "$OUTPUT_DIR/mouse-cadence-$(date +%Y%m%d-%H%M%S).txt"
