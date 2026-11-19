#!/usr/bin/env sh
# Install the `lt` CLI from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Bipolaire/load-testing/develop/scripts/install.sh | sh
#   curl -fsSL ... | LT_VERSION=v0.1.1 sh
#
# Env:
#   LT_REPO         owner/repo (default: Bipolaire/load-testing)
#   LT_VERSION      tag (e.g. v0.1.1); default: latest release
#   LT_INSTALL_DIR  install directory (default: /usr/local/bin or ~/.local/bin)
#   LT_FORCE        set to 1 to overwrite an existing binary
#   GITHUB_TOKEN    optional; needed only for private repos/releases

set -eu

DEFAULT_REPO="Bipolaire/load-testing"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
info() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

REPO="${LT_REPO:-${GITHUB_REPOSITORY:-$DEFAULT_REPO}}"
[ -n "$REPO" ] || die "set LT_REPO=owner/repo (GitHub repository that hosts Releases)"

need_cmd curl
need_cmd uname
need_cmd mktemp

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
  linux) OS_KEY=unknown-linux-musl ;;
  darwin) OS_KEY=apple-darwin ;;
  msys*|cygwin*|mingw*|windows_nt)
    die "use scripts/install.ps1 on Windows"
    ;;
  *) die "unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH_KEY=x86_64 ;;
  aarch64|arm64) ARCH_KEY=aarch64 ;;
  *) die "unsupported architecture: $ARCH" ;;
esac

TARGET="${ARCH_KEY}-${OS_KEY}"
VERSION="${LT_VERSION:-}"

resolve_version() {
  if [ -n "$VERSION" ]; then
    case "$VERSION" in
      v*) printf '%s\n' "$VERSION" ;;
      *) printf 'v%s\n' "$VERSION" ;;
    esac
    return
  fi
  need_cmd sed
  tag="$(
    curl -fsSL ${GITHUB_TOKEN:+-H "Authorization: Bearer $GITHUB_TOKEN"} \
      "https://api.github.com/repos/${REPO}/releases/latest" \
      | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
      | head -n 1
  )"
  [ -n "$tag" ] || die "could not resolve latest release for ${REPO}"
  printf '%s\n' "$tag"
}

VERSION="$(resolve_version)"
ARCHIVE="lt-${VERSION}-${TARGET}.tar.gz"
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
DOWNLOAD_URL="${BASE_URL}/${ARCHIVE}"
CHECKSUM_URL="${BASE_URL}/SHA256SUMS"

INSTALL_DIR="${LT_INSTALL_DIR:-}"
if [ -z "$INSTALL_DIR" ]; then
  if [ -w /usr/local/bin ] 2>/dev/null || [ "$(id -u)" -eq 0 ]; then
    INSTALL_DIR=/usr/local/bin
  else
    INSTALL_DIR="${HOME}/.local/bin"
  fi
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

info "Downloading ${ARCHIVE}"
if ! curl -fsSL ${GITHUB_TOKEN:+-H "Authorization: Bearer $GITHUB_TOKEN"} \
  -o "${TMP}/${ARCHIVE}" "$DOWNLOAD_URL"; then
  die "download failed: ${DOWNLOAD_URL}
hint: private repo needs GITHUB_TOKEN; also ensure a Release tag exists with CLI assets (push tag v0.1.0)"
fi

if curl -fsSL ${GITHUB_TOKEN:+-H "Authorization: Bearer $GITHUB_TOKEN"} \
  -o "${TMP}/SHA256SUMS" "$CHECKSUM_URL" 2>/dev/null; then
  need_cmd awk
  expected="$(awk -v f="$ARCHIVE" '$2 == f { print $1; exit }' "${TMP}/SHA256SUMS")"
  if [ -n "$expected" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
      actual="$(sha256sum "${TMP}/${ARCHIVE}" | awk '{ print $1 }')"
    elif command -v shasum >/dev/null 2>&1; then
      actual="$(shasum -a 256 "${TMP}/${ARCHIVE}" | awk '{ print $1 }')"
    else
      warn "no sha256 tool found; skipping checksum verification"
      actual="$expected"
    fi
    [ "$actual" = "$expected" ] || die "checksum mismatch for ${ARCHIVE}"
    info "Checksum OK"
  else
    warn "no checksum entry for ${ARCHIVE} in SHA256SUMS"
  fi
else
  warn "SHA256SUMS not found; skipping checksum verification"
fi

need_cmd tar
tar -C "$TMP" -xzf "${TMP}/${ARCHIVE}"
[ -f "${TMP}/lt" ] || die "archive did not contain lt binary"

mkdir -p "$INSTALL_DIR"
DEST="${INSTALL_DIR}/lt"
if [ -e "$DEST" ] && [ "${LT_FORCE:-0}" != "1" ]; then
  if [ ! -w "$DEST" ] && [ ! -w "$INSTALL_DIR" ]; then
    die "cannot write ${DEST} (try sudo or LT_INSTALL_DIR=...)"
  fi
fi

# Prefer install(1) when available for atomic + mode bits.
if command -v install >/dev/null 2>&1; then
  install -m 755 "${TMP}/lt" "$DEST"
else
  cp "${TMP}/lt" "$DEST"
  chmod 755 "$DEST"
fi

bold "Installed lt ${VERSION} → ${DEST}"
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    warn "${INSTALL_DIR} is not on PATH"
    printf 'Add this to your shell profile:\n  export PATH="%s:\$PATH"\n' "$INSTALL_DIR"
    ;;
esac

"$DEST" --version || true
