#!/usr/bin/env bash
set -euo pipefail

# Build a debug Android APK and optionally install it via adb.
# Usage: ./build-debug.sh [--install]

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

# ─── Preflight checks ────────────────────────────────────────

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

# ─── Build debug APK ─────────────────────────────────────────

echo "==> Building rshare-app (Android debug APK)..."
cargo apk build -p rshare-app --lib --no-default-features --features android

mkdir -p "$DIST"

APK_PATH="target/debug/apk/rshare-app.apk"
APK_NAME="rshare-app-debug.apk"

if [ -f "$APK_PATH" ]; then
    cp "$APK_PATH" "$DIST/$APK_NAME"
    echo "    -> $DIST/$APK_NAME"
else
    echo "Error: APK not found at $APK_PATH"
    exit 1
fi

# ─── Optional: install via adb ───────────────────────────────

if [ "${1:-}" = "--install" ]; then
    if ! command -v adb &>/dev/null; then
        echo "Error: adb not found. Install with: sudo pacman -S android-tools"
        exit 1
    fi
    echo "==> Installing on device..."
    adb install -r "$DIST/$APK_NAME"
    echo "    Done. Launch com.rshare.app on your device."
fi
