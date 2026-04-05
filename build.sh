#!/usr/bin/env bash
set -euo pipefail

DIST="dist"
ANDROID_TARGET="aarch64-linux-android"

# Read version from workspace Cargo.toml
VERSION=$(grep -m1 'version = ' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')

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
    echo "Usage: $0 [desktop|windows|android|server|all]"
    echo ""
    echo "Commands:"
    echo "  desktop   Build server + CLI + desktop GUI for Linux (release)"
    echo "  windows   Cross-compile server + CLI + desktop GUI for Windows (release)"
    echo "  android   Build Android APK (requires Android NDK)"
    echo "  server    Build server only (release)"
    echo "  all       Build everything (linux + windows + android)"
    echo ""
    echo "Output goes to dist/"
    echo "Version: $VERSION"
    exit 1
}

# ─── Linux builds ──────────────────────────────────────────────

build_server() {
    local target="${1:-}"
    local target_flag=""
    local bin_dir="target/release"
    local ext=""

    if [ -n "$target" ]; then
        target_flag="--target $target"
        bin_dir="target/$target/release"
    fi
    if [[ "$target" == *windows* ]]; then
        ext=".exe"
    fi

    echo "==> Building rshare-server${target:+ ($target)}..."
    cargo build -p rshare-server --release $target_flag
    mkdir -p "$DIST"
    cp "$bin_dir/rshare-server$ext" "$DIST/"
    echo "    -> $DIST/rshare-server$ext"
}

build_cli() {
    local target="${1:-}"
    local target_flag=""
    local bin_dir="target/release"
    local ext=""

    if [ -n "$target" ]; then
        target_flag="--target $target"
        bin_dir="target/$target/release"
    fi
    if [[ "$target" == *windows* ]]; then
        ext=".exe"
    fi

    echo "==> Building rshare-cli${target:+ ($target)}..."
    cargo build -p rshare-cli --release $target_flag
    mkdir -p "$DIST"
    cp "$bin_dir/rshare-cli$ext" "$DIST/"
    echo "    -> $DIST/rshare-cli$ext"
}

build_desktop_app() {
    local target="${1:-}"
    local target_flag=""
    local bin_dir="target/release"
    local ext=""

    if [ -n "$target" ]; then
        target_flag="--target $target"
        bin_dir="target/$target/release"
    fi
    if [[ "$target" == *windows* ]]; then
        ext=".exe"
    fi

    echo "==> Building rshare-app (desktop)${target:+ ($target)}..."
    cargo build -p rshare-app --release $target_flag
    mkdir -p "$DIST"
    cp "$bin_dir/rshare-app$ext" "$DIST/"
    echo "    -> $DIST/rshare-app$ext"
}

# ─── Package helpers ───────────────────────────────────────────

# Pack CLIs into tar.gz/zip, leave GUI app as standalone binary
package_dist() {
    local os="$1"   # linux, windows
    local arch="$2" # x86_64, etc.
    local ext=""
    local archive_ext="tar.gz"

    if [ "$os" = "windows" ]; then
        ext=".exe"
        archive_ext="zip"
    fi

    local tag="v${VERSION}-${os}-${arch}"

    # Pack CLIs
    local cli_archive="rshare-cli-${tag}.${archive_ext}"
    echo "==> Packaging CLIs -> $cli_archive"
    if [ "$os" = "windows" ]; then
        (cd "$DIST" && zip -q "$cli_archive" "rshare-server${ext}" "rshare-cli${ext}")
    else
        tar -czf "$DIST/$cli_archive" -C "$DIST" "rshare-server${ext}" "rshare-cli${ext}"
    fi
    rm "$DIST/rshare-server${ext}" "$DIST/rshare-cli${ext}"

    # Rename GUI app with version tag
    local app_name="rshare-app-${tag}${ext}"
    mv "$DIST/rshare-app${ext}" "$DIST/$app_name"

    echo "    -> $DIST/$cli_archive"
    echo "    -> $DIST/$app_name"
}

# ─── Android ───────────────────────────────────────────────────

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
    local apk_name="rshare-app-v${VERSION}-android.apk"
    if [ -f "$apk_path" ]; then
        cp "$apk_path" "$DIST/$apk_name"
        echo "    -> $DIST/$apk_name"
    fi
}

# ─── Composite targets ────────────────────────────────────────

build_desktop() {
    build_server
    build_cli
    build_desktop_app

    local arch
    arch=$(uname -m)
    package_dist "linux" "$arch"

    echo ""
    echo "Desktop build complete (v$VERSION). Output in $DIST/"
}

build_windows() {
    local target="x86_64-pc-windows-gnu"

    if ! rustup target list --installed | grep -q "^${target}$"; then
        echo "Error: Target $target not installed."
        echo "Run: rustup target add $target"
        exit 1
    fi

    build_server "$target"
    build_cli "$target"
    build_desktop_app "$target"

    package_dist "windows" "x86_64"

    echo ""
    echo "Windows build complete (v$VERSION). Output in $DIST/"
}

build_all() {
    build_desktop
    echo ""
    build_windows
    echo ""
    build_android
    echo ""
    echo "All builds complete (v$VERSION). Output in $DIST/"
}

# ─── Entry point ───────────────────────────────────────────────

CMD="${1:-}"

case "$CMD" in
    desktop) build_desktop ;;
    windows) build_windows ;;
    android) build_android ;;
    server)  build_server ;;
    all)     build_all ;;
    *)       usage ;;
esac
