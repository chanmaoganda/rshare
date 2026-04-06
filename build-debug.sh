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

# ─── aapt wrapper for icon resources ─────────────────────────

setup_aapt_wrapper() {
    local res_dir
    res_dir=$(realpath "crates/rshare-app/res")
    local sdk="${ANDROID_HOME:-/opt/android-sdk}"
    local build_tools
    build_tools=$(ls -1d "$sdk/build-tools/"* 2>/dev/null | sort -V | tail -1)
    local real_aapt="$build_tools/aapt"

    AAPT_WRAPPER_DIR=$(mktemp -d)
    cat > "$AAPT_WRAPPER_DIR/aapt" <<WRAPPER
#!/bin/bash
if [ "\$1" = "package" ] && [ -d "$res_dir" ]; then
    exec "$real_aapt" "\$1" -S "$res_dir" "\${@:2}"
else
    exec "$real_aapt" "\$@"
fi
WRAPPER
    chmod +x "$AAPT_WRAPPER_DIR/aapt"
    export PATH="$AAPT_WRAPPER_DIR:$PATH"
    echo "    aapt wrapper installed (res: $res_dir)"
}

teardown_aapt_wrapper() {
    if [ -n "${AAPT_WRAPPER_DIR:-}" ] && [ -d "$AAPT_WRAPPER_DIR" ]; then
        rm -rf "$AAPT_WRAPPER_DIR"
        unset AAPT_WRAPPER_DIR
    fi
}

# ─── Build debug APK ─────────────────────────────────────────

echo "==> Building rshare-app (Android debug APK)..."

if [ -d "crates/rshare-app/res" ]; then
    setup_aapt_wrapper
fi

cargo apk build -p rshare-app --lib --no-default-features --features android

teardown_aapt_wrapper

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
