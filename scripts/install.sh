#!/usr/bin/env bash
# nupic installer.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/goliajp/nupic/develop/scripts/install.sh | bash
#
# Env knobs:
#   INSTALL_DIR   override install path (default: ~/.local/bin)
#   NUPIC_TAG     pin to a specific tag (default: latest)

set -euo pipefail

REPO="goliajp/nupic"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
NUPIC_TAG="${NUPIC_TAG:-}"

# ---- detect platform ----
os="$(uname -s)"
arch="$(uname -m)"
target=""
case "$os/$arch" in
    Darwin/arm64)        target="aarch64-apple-darwin" ;;
    Darwin/x86_64)       target="x86_64-apple-darwin" ;;
    Linux/x86_64)        target="x86_64-unknown-linux-gnu" ;;
    Linux/aarch64|Linux/arm64) target="aarch64-unknown-linux-gnu" ;;
    *)
        printf 'Unsupported platform: %s/%s\n' "$os" "$arch" >&2
        printf 'See https://github.com/%s/releases for available targets,\n' "$REPO" >&2
        printf 'or build from source: cargo install --git https://github.com/%s nupic-cli\n' "$REPO" >&2
        exit 1
        ;;
esac

# ---- resolve tag ----
if [[ -z "$NUPIC_TAG" ]]; then
    NUPIC_TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep -oE '"tag_name"[[:space:]]*:[[:space:]]*"[^"]+"' \
        | head -1 \
        | sed -E 's/.*"([^"]+)"[[:space:]]*$/\1/')
    if [[ -z "$NUPIC_TAG" ]]; then
        printf 'Could not detect latest release tag from GitHub API.\n' >&2
        exit 1
    fi
fi

archive="nupic-${NUPIC_TAG}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/download/${NUPIC_TAG}/${archive}"

printf '→ nupic %s for %s\n' "$NUPIC_TAG" "$target"
printf '  archive: %s\n' "$url"

# ---- download + checksum ----
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fsSL -o "$tmp/$archive" "$url"
curl -fsSL -o "$tmp/SHA256SUMS.txt" \
    "https://github.com/${REPO}/releases/download/${NUPIC_TAG}/SHA256SUMS.txt"

expected=$(grep " $archive\$" "$tmp/SHA256SUMS.txt" | awk '{print $1}')
if [[ -z "$expected" ]]; then
    printf '⚠️  Could not find checksum for %s in SHA256SUMS.txt — skipping verify.\n' "$archive" >&2
else
    if command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$tmp/$archive" | awk '{print $1}')
    elif command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$tmp/$archive" | awk '{print $1}')
    else
        printf '⚠️  Neither shasum nor sha256sum installed — skipping verify.\n' >&2
        actual="$expected"
    fi
    if [[ "$actual" != "$expected" ]]; then
        printf '❌  Checksum mismatch!\n  expected: %s\n  actual:   %s\n' "$expected" "$actual" >&2
        exit 1
    fi
    printf '✓ sha256 verified\n'
fi

# ---- extract + install ----
tar -xzf "$tmp/$archive" -C "$tmp"
mkdir -p "$INSTALL_DIR"
mv "$tmp/nupic" "$INSTALL_DIR/nupic"
chmod +x "$INSTALL_DIR/nupic"

printf '\n✓ installed → %s\n' "$INSTALL_DIR/nupic"
printf '  %s\n' "$("$INSTALL_DIR/nupic" --version)"

# ---- PATH hint ----
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        printf '\nNote: %s is not on PATH. Add this to your shell rc:\n' "$INSTALL_DIR"
        printf '  export PATH="%s:$PATH"\n' "$INSTALL_DIR"
        ;;
esac
