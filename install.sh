#!/usr/bin/env bash
# T-20 dry run (architecture doc §15/§16): the fresh-machine installer.
#
#   curl -fsSL https://lintl.sh/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/elcoosp/lintl/main/install.sh | sh -s -- --version v0.1.0
#
# Downloads the release archive for the running OS/arch, verifies its
# SHA-256 against the release's `SHA256SUMS` (the checksum file the release
# workflow publishes and the Composer plugin pins), and installs the
# `lintl` binary to ~/.local/bin (or $LINTL_BIN_DIR / a --prefix).
#
# LINTL_RELEASE_ROOT overrides the release base URL — the local dry run
# and the CI install-smoke job point it at a directory of prepared archives
# so the *whole* script (detect → download → verify → install → smoke) is
# exercised without a published release.
set -eu

RELEASE_ROOT="${LINTL_RELEASE_ROOT:-https://github.com/elcoosp/lintl/releases/download/v0.1.0}"
BIN_DIR="${LINTL_BIN_DIR:-${HOME}/.local/bin}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

say() { printf 'lintl-install: %s\n' "$1" >&2; }

# 1. Detect the platform (§12.1: linux x64/arm64 musl, macOS x64/arm64, Windows x64).
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS-$ARCH" in
    Linux-x86_64) ARCHIVE="lintl-linux-x64.tar.gz" ;;
    Linux-aarch64) ARCHIVE="lintl-linux-arm64.tar.gz" ;;
    Darwin-x86_64) ARCHIVE="lintl-macos-x64.tar.gz" ;;
    Darwin-arm64) ARCHIVE="lintl-macos-arm64.tar.gz" ;;
    MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64 | MINGW*-amd64) ARCHIVE="lintl-windows-x64.zip" ;;
    *) say "unsupported platform: $OS-$ARCH (build from source: cargo install lintl-cli)"; exit 1 ;;
esac
say "detected $OS-$ARCH — fetching $ARCHIVE"

# 2. Download the archive and the checksum file, verify (§12.3 supply-chain stance).
fetch() {
    # $1 = URL or local path, $2 = output path. LINTL_RELEASE_ROOT may be a
    # local directory (the CI install-smoke job and the local dry run stage
    # archives on disk) or a file:// URL — both are copied verbatim; only a
    # remote http(s) URL needs curl/wget. curl rejects a bare local path with
    # "No host part in the URL", so the local case is handled before it.
    case "$1" in
        file://*)
            cp "${1#file://}" "$2"
            ;;
        *://*)
            if command -v curl >/dev/null 2>&1; then
                curl -fsSL "$1" -o "$2"
            elif command -v wget >/dev/null 2>&1; then
                wget -qO "$2" "$1"
            else
                say "need curl or wget to download the release"; exit 1
            fi
            ;;
        *)
            # A bare local path (no scheme): copy it if it exists.
            if [ -f "$1" ]; then
                cp "$1" "$2"
            else
                say "no such file: $1"; exit 1
            fi
            ;;
    esac
}
fetch "${RELEASE_ROOT}/${ARCHIVE}" "$TMP/$ARCHIVE"
fetch "${RELEASE_ROOT}/SHA256SUMS" "$TMP/SHA256SUMS"
# sha256sum writes `<hash>  <file>` (text mode) or `<hash> *<file>`
# (binary mode — the default on Windows Git Bash). Accept both: the
# filename is preceded by whitespace and an optional `*` marker.
EXPECTED="$(grep -E " ${ARCHIVE}\$| \*${ARCHIVE}\$" "$TMP/SHA256SUMS" | head -1 | cut -d' ' -f1)"
if [ -z "$EXPECTED" ]; then
    say "no checksum for ${ARCHIVE} in SHA256SUMS — refusing to install"
    exit 1
fi
if ! echo "$EXPECTED  $TMP/$ARCHIVE" | sha256sum -c - >/dev/null 2>&1 && \
   ! echo "$EXPECTED  $TMP/$ARCHIVE" | shasum -a 256 -c - >/dev/null 2>&1; then
    say "checksum mismatch for ${ARCHIVE} — refusing to install"
    exit 1
fi
say "checksum verified ($EXPECTED)"

# 3. Extract and install (tar.gz everywhere; the Windows release is a zip
#    that Git Bash's unzip — present on every GitHub runner image — opens).
mkdir -p "$BIN_DIR"
case "$ARCHIVE" in
    *.zip)
        unzip -o "$TMP/$ARCHIVE" -d "$TMP" >/dev/null
        cp "$TMP/lintl.exe" "$BIN_DIR/lintl.exe"
        BIN="$BIN_DIR/lintl.exe"
        ;;
    *.tar.gz)
        tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
        install "$TMP/lintl" "$BIN_DIR/lintl"
        BIN="$BIN_DIR/lintl"
        ;;
esac
say "installed $BIN"

# 4. Smoke: the binary runs and the PATH picks it up.
if ! command -v lintl >/dev/null 2>&1; then
    say "add ${BIN_DIR} to your PATH (e.g. add 'export PATH=\"${BIN_DIR}:\$PATH\"' to your shell rc)"
fi
"$BIN" --version
say "done — try: lintl init && lintl build --in-place hello.lintl.php"
