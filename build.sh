#!/usr/bin/env bash
set -euo pipefail

DIST="dist"
ANDROID_TARGET="aarch64-linux-android"

resolve_android_jar() {
    if [ -z "${ANDROID_JAR:-}" ]; then
        local sdk="${ANDROID_HOME:-/opt/android-sdk}"
        local platform
        platform=$(ls -1d "$sdk/platforms/android-"* 2>/dev/null | sort -V | tail -1)
        if [ -n "$platform" ] && [ -f "$platform/android.jar" ]; then
            export ANDROID_JAR="$platform/android.jar"
            echo "    Using ANDROID_JAR=$ANDROID_JAR"
        fi
    fi
}

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

    if ! command -v cargo-apk &>/dev/null; then
        echo "Error: cargo-apk not found. Install with: cargo install cargo-apk"
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

    resolve_android_jar

    cargo apk build -p rshare-app --lib --no-default-features --features android --release
    mkdir -p "$DIST"

    local apk_path="target/release/apk/rshare-app.apk"
    if [ -f "$apk_path" ]; then
        cp "$apk_path" "$DIST/"
        echo "    -> $DIST/rshare-app.apk"
    fi
}

build_desktop() {
    build_server
    build_cli
    build_desktop_app

    # Pack into tar.gz
    echo "==> Packaging binaries..."
    local archive="rshare-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
    tar -czf "$DIST/$archive.tar.gz" -C "$DIST" rshare-server rshare-cli rshare-app
    rm "$DIST/rshare-server" "$DIST/rshare-cli" "$DIST/rshare-app"

    echo ""
    echo "Desktop build complete: $DIST/$archive.tar.gz"
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
