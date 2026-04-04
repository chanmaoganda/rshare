#!/usr/bin/env bash
set -euo pipefail

DIST="dist"
ANDROID_TARGET="aarch64-linux-android"

usage() {
    echo "Usage: $0 [desktop|android|server|all]"
    echo ""
    echo "Commands:"
    echo "  desktop   Build server + CLI + desktop GUI (release)"
    echo "  android   Build Android APK (requires Android NDK)"
    echo "  server    Build server only (release)"
    echo "  all       Build everything (desktop + android)"
    echo ""
    echo "Output goes to dist/"
    exit 1
}

build_server() {
    echo "==> Building rshare-server..."
    cargo build -p rshare-server --release
    mkdir -p "$DIST"
    cp target/release/rshare-server "$DIST/"
    echo "    -> $DIST/rshare-server"
}

build_cli() {
    echo "==> Building rshare-cli..."
    cargo build -p rshare-cli --release
    mkdir -p "$DIST"
    cp target/release/rshare-cli "$DIST/"
    echo "    -> $DIST/rshare-cli"
}

build_desktop_app() {
    echo "==> Building rshare-app (desktop)..."
    cargo build -p rshare-app --release
    mkdir -p "$DIST"
    cp target/release/rshare-app "$DIST/"
    echo "    -> $DIST/rshare-app"
}

build_android() {
    echo "==> Building rshare-app (Android APK)..."

    if ! command -v cargo-ndk &>/dev/null; then
        echo "Error: cargo-ndk not found. Install with: cargo install cargo-ndk"
        exit 1
    fi

    if [ -z "${ANDROID_NDK_HOME:-}" ]; then
        echo "Error: ANDROID_NDK_HOME not set"
        exit 1
    fi

    if ! rustup target list --installed | grep -q "$ANDROID_TARGET"; then
        echo "Error: Target $ANDROID_TARGET not installed."
        echo "Run: rustup target add $ANDROID_TARGET"
        exit 1
    fi

    cargo ndk -t arm64-v8a build -p rshare-app --features android --release
    mkdir -p "$DIST"

    local so_path="target/$ANDROID_TARGET/release/librshare_app.so"
    if [ -f "$so_path" ]; then
        cp "$so_path" "$DIST/"
        echo "    -> $DIST/librshare_app.so"
    fi

    echo "    Note: To produce a full APK, use cargo-apk or integrate with a Gradle project."
}

build_desktop() {
    build_server
    build_cli
    build_desktop_app
    echo ""
    echo "Desktop build complete. Binaries in $DIST/"
}

build_all() {
    build_desktop
    echo ""
    build_android
    echo ""
    echo "All builds complete. Output in $DIST/"
}

CMD="${1:-}"

case "$CMD" in
    desktop) build_desktop ;;
    android) build_android ;;
    server)  build_server ;;
    all)     build_all ;;
    *)       usage ;;
esac
