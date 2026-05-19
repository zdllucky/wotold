#!/usr/bin/env bash
# Build macOS audio sidecar и положить в apps/desktop/src-tauri/binaries/
# по Tauri-конвенции: wotold-audio-<target-triple>.
#
# См. M1.2 паспорта и apps/desktop/sidecars/macos-audio/README.md.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SIDECAR_DIR="$ROOT/apps/desktop/sidecars/macos-audio"
TARGET_DIR="$ROOT/apps/desktop/src-tauri/binaries"

# rustc нужен только для --print host (определить target-triple).
# Подхватываем cargo env если он не в PATH (типично если script зовётся не из login-shell).
if ! command -v rustc >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi

if ! command -v rustc >/dev/null 2>&1; then
  echo "build-audio-sidecar: rustc not found (нужен для определения target-triple)" >&2
  exit 1
fi

ARCH="$(rustc -vV | awk -F': ' '/^host/ {print $2}')"

if [[ "$ARCH" != *-apple-darwin ]]; then
  echo "build-audio-sidecar: not a macOS host ($ARCH), skipping" >&2
  exit 0
fi

echo "Building wotold-audio sidecar for $ARCH"
cd "$SIDECAR_DIR"
swift build -c release

mkdir -p "$TARGET_DIR"
cp .build/release/WotoldAudio "$TARGET_DIR/wotold-audio-$ARCH"
chmod +x "$TARGET_DIR/wotold-audio-$ARCH"
echo "→ $TARGET_DIR/wotold-audio-$ARCH"
