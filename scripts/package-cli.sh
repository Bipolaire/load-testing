#!/usr/bin/env bash
# Build and package the `lt` CLI for the current host (or TARGET=...).
#
# Usage:
#   ./scripts/package-cli.sh
#   TARGET=x86_64-unknown-linux-musl ./scripts/package-cli.sh
#   VERSION=0.1.0 ./scripts/package-cli.sh
#
# Output: dist/cli/lt-v<version>-<target>.{tar.gz|zip} and matching .sha256

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="${VERSION:-}"
if [[ -z "$VERSION" ]]; then
  VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
fi
VERSION="${VERSION#v}"

if [[ -n "${TARGET:-}" ]]; then
  HOST_TARGET="$TARGET"
else
  HOST_TARGET="$(rustc -vV | awk '/^host:/{print $2}')"
fi

OUT_DIR="${OUT_DIR:-dist/cli}"
mkdir -p "$OUT_DIR"

echo "==> Building lt for ${HOST_TARGET} (v${VERSION})"

case "$HOST_TARGET" in
  *-unknown-linux-musl)
    if ! command -v musl-gcc >/dev/null 2>&1 && ! command -v x86_64-linux-musl-gcc >/dev/null 2>&1; then
      echo "warning: musl-tools may be required (apt install musl-tools)" >&2
    fi
    ;;
esac

rustup target add "$HOST_TARGET" >/dev/null 2>&1 || true
cargo build --release --target "$HOST_TARGET" -p lt-cli

STAGING="$(mktemp -d)"
cleanup() { rm -rf "$STAGING"; }
trap cleanup EXIT

if [[ "$HOST_TARGET" == *windows* ]]; then
  BIN="target/${HOST_TARGET}/release/lt.exe"
  [[ -f "$BIN" ]] || { echo "error: missing $BIN" >&2; exit 1; }
  cp "$BIN" "$STAGING/lt.exe"
  ARCHIVE="lt-v${VERSION}-${HOST_TARGET}.zip"
  (
    cd "$STAGING"
    if command -v zip >/dev/null 2>&1; then
      zip -q "$ROOT/$OUT_DIR/$ARCHIVE" lt.exe
    else
      powershell -NoProfile -Command "Compress-Archive -Path lt.exe -DestinationPath '$ROOT/$OUT_DIR/$ARCHIVE'"
    fi
  )
else
  BIN="target/${HOST_TARGET}/release/lt"
  [[ -f "$BIN" ]] || { echo "error: missing $BIN" >&2; exit 1; }
  cp "$BIN" "$STAGING/lt"
  chmod +x "$STAGING/lt"
  ARCHIVE="lt-v${VERSION}-${HOST_TARGET}.tar.gz"
  tar -C "$STAGING" -czf "$OUT_DIR/$ARCHIVE" lt
fi

(
  cd "$OUT_DIR"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$ARCHIVE" > "${ARCHIVE}.sha256"
  else
    shasum -a 256 "$ARCHIVE" > "${ARCHIVE}.sha256"
  fi
)

echo "==> Wrote ${OUT_DIR}/${ARCHIVE}"
echo "==> Wrote ${OUT_DIR}/${ARCHIVE}.sha256"
