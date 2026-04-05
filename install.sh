#!/usr/bin/env bash
set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
BIN_DIR="$PREFIX/bin"
SHARE_DIR="$PREFIX/share"

usage() {
    echo "Usage: $0 [options]"
    echo ""
    echo "Install rshare binaries from dist/ to system PATH."
    echo ""
    echo "Options:"
    echo "  --prefix DIR   Install prefix (default: /usr/local)"
    echo "  --uninstall    Remove installed binaries"
    echo "  -h, --help     Show this help"
    echo ""
    echo "Installs: rshare-server, rshare-cli, rshare-app"
    exit 0
}

UNINSTALL=false

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)   PREFIX="$2"; BIN_DIR="$PREFIX/bin"; SHARE_DIR="$PREFIX/share"; shift 2 ;;
        --uninstall) UNINSTALL=true; shift ;;
        -h|--help)  usage ;;
        *)          echo "Unknown option: $1"; usage ;;
    esac
done

BINS=(rshare-server rshare-cli rshare-app)

if [ "$UNINSTALL" = true ]; then
    echo "==> Uninstalling rshare..."
    for bin in "${BINS[@]}"; do
        if [ -f "$BIN_DIR/$bin" ]; then
            rm -f "$BIN_DIR/$bin"
            echo "    Removed $BIN_DIR/$bin"
        fi
    done
    rm -f "$SHARE_DIR/applications/rshare.desktop"
    rm -f "$SHARE_DIR/icons/hicolor/scalable/apps/rshare.svg"
    echo "    Removed desktop entry and icon"
    echo "Done."
    exit 0
fi

# ─── Find binaries ─────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DIST_DIR="$SCRIPT_DIR/dist"

# Locate CLI archive
CLI_TAR=$(find "$DIST_DIR" -name 'rshare-cli-*-linux-*.tar.gz' 2>/dev/null | head -1)
# Locate desktop app binary
APP_BIN=$(find "$DIST_DIR" -name 'rshare-app-*-linux-*' ! -name '*.tar.gz' 2>/dev/null | head -1)

if [ -z "$CLI_TAR" ]; then
    echo "Error: No Linux CLI archive found in $DIST_DIR"
    echo "Run ./build.sh desktop first."
    exit 1
fi

if [ -z "$APP_BIN" ]; then
    echo "Error: No Linux desktop app found in $DIST_DIR"
    echo "Run ./build.sh desktop first."
    exit 1
fi

# ─── Install ───────────────────────────────────────────────────

echo "==> Installing rshare to $BIN_DIR..."

# Extract CLI binaries from archive
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

tar -xzf "$CLI_TAR" -C "$TMPDIR"

for bin in rshare-server rshare-cli; do
    if [ -f "$TMPDIR/$bin" ]; then
        install -Dm755 "$TMPDIR/$bin" "$BIN_DIR/$bin"
        echo "    Installed $BIN_DIR/$bin"
    else
        echo "    Warning: $bin not found in archive"
    fi
done

# Install desktop app
install -Dm755 "$APP_BIN" "$BIN_DIR/rshare-app"
echo "    Installed $BIN_DIR/rshare-app"

# ─── Desktop entry & icon ─────────────────────────────────────

ASSETS_DIR="$SCRIPT_DIR/assets"

if [ -f "$ASSETS_DIR/rshare.desktop" ]; then
    install -Dm644 "$ASSETS_DIR/rshare.desktop" "$SHARE_DIR/applications/rshare.desktop"
    echo "    Installed $SHARE_DIR/applications/rshare.desktop"
fi

if [ -f "$ASSETS_DIR/rshare.svg" ]; then
    install -Dm644 "$ASSETS_DIR/rshare.svg" "$SHARE_DIR/icons/hicolor/scalable/apps/rshare.svg"
    echo "    Installed $SHARE_DIR/icons/hicolor/scalable/apps/rshare.svg"
fi

# Update icon cache if available
if command -v gtk-update-icon-cache &>/dev/null; then
    gtk-update-icon-cache -f -t "$SHARE_DIR/icons/hicolor" 2>/dev/null || true
fi

echo ""
echo "Done. Run 'rshare-server', 'rshare-cli', or 'rshare-app'."
