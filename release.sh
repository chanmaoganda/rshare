#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version> [--android]"
    echo ""
    echo "Examples:"
    echo "  $0 0.1.0            # Desktop release only"
    echo "  $0 0.1.0 --android  # Desktop + Android"
    echo ""
    echo "Produces:"
    echo "  release/rshare-<version>-<target>.tar.gz   (Linux/macOS)"
    echo "  release/rshare-<version>-<target>.zip       (Windows cross)"
    echo "  release/rshare-<version>-android.tar.gz     (Android .so)"
    echo "  release/SHA256SUMS.txt"
    exit 1
fi

BUILD_ANDROID=false
if [ "${2:-}" = "--android" ]; then
    BUILD_ANDROID=true
fi

RELEASE_DIR="release"
HOST_TARGET=$(rustc -vV | grep '^host:' | awk '{print $2}')
DESKTOP_BINS=(rshare-server rshare-cli rshare-app)

rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"

# ─── Desktop build ──────────────────────────────────────────────
echo "==> Building desktop release for $HOST_TARGET..."
cargo build --release

ARCHIVE_NAME="rshare-${VERSION}-${HOST_TARGET}"
STAGING="$RELEASE_DIR/$ARCHIVE_NAME"
mkdir -p "$STAGING"

for bin in "${DESKTOP_BINS[@]}"; do
    src="target/release/$bin"
    if [ -f "$src" ]; then
        cp "$src" "$STAGING/"
        echo "    + $bin"
    else
        echo "    ! $bin not found, skipping"
    fi
done

# Include docs
cp README.md "$STAGING/" 2>/dev/null || true

# Create archive
echo "==> Packaging $ARCHIVE_NAME.tar.gz..."
tar -czf "$RELEASE_DIR/$ARCHIVE_NAME.tar.gz" -C "$RELEASE_DIR" "$ARCHIVE_NAME"
rm -rf "$STAGING"

# ─── Cross-compile targets (optional, if installed) ────────────
CROSS_TARGETS=(
    "x86_64-unknown-linux-musl"
    "aarch64-unknown-linux-gnu"
)

for target in "${CROSS_TARGETS[@]}"; do
    if [ "$target" = "$HOST_TARGET" ]; then
        continue
    fi

    if ! rustup target list --installed | grep -q "^${target}$"; then
        echo "==> Skipping $target (not installed)"
        continue
    fi

    echo "==> Cross-compiling for $target..."
    if cargo build --release --target "$target" 2>/dev/null; then
        ARCHIVE_NAME="rshare-${VERSION}-${target}"
        STAGING="$RELEASE_DIR/$ARCHIVE_NAME"
        mkdir -p "$STAGING"

        for bin in "${DESKTOP_BINS[@]}"; do
            src="target/$target/release/$bin"
            if [ -f "$src" ]; then
                cp "$src" "$STAGING/"
            fi
        done

        cp README.md "$STAGING/" 2>/dev/null || true
        tar -czf "$RELEASE_DIR/$ARCHIVE_NAME.tar.gz" -C "$RELEASE_DIR" "$ARCHIVE_NAME"
        rm -rf "$STAGING"
        echo "    -> $ARCHIVE_NAME.tar.gz"
    else
        echo "    ! Cross-compile failed for $target, skipping"
    fi
done

# ─── Android build ──────────────────────────────────────────────
if [ "$BUILD_ANDROID" = true ]; then
    ANDROID_TARGET="aarch64-linux-android"

    if ! command -v cargo-apk &>/dev/null; then
        echo "==> Skipping Android (cargo-apk not found)"
    elif [ -z "${ANDROID_NDK_HOME:-}" ]; then
        echo "==> Skipping Android (ANDROID_NDK_HOME not set)"
    elif ! rustup target list --installed | grep -q "^${ANDROID_TARGET}$"; then
        echo "==> Skipping Android (target $ANDROID_TARGET not installed)"
    else
        echo "==> Building Android APK ($ANDROID_TARGET)..."

        # Resolve ANDROID_JAR for Slint's build script
        if [ -z "${ANDROID_JAR:-}" ]; then
            sdk="${ANDROID_HOME:-/opt/android-sdk}"
            platform=$(ls -1d "$sdk/platforms/android-"* 2>/dev/null | sort -V | tail -1)
            if [ -n "$platform" ] && [ -f "$platform/android.jar" ]; then
                export ANDROID_JAR="$platform/android.jar"
            fi
        fi

        cargo apk build -p rshare-app --lib --no-default-features --features android --release

        apk_path="target/release/apk/rshare-app.apk"
        if [ -f "$apk_path" ]; then
            cp "$apk_path" "$RELEASE_DIR/rshare-${VERSION}-android.apk"
            echo "    -> rshare-${VERSION}-android.apk"
        fi
    fi
fi

# ─── Checksums ──────────────────────────────────────────────────
echo "==> Generating checksums..."
cd "$RELEASE_DIR"
sha256sum *.tar.gz *.zip *.apk 2>/dev/null > SHA256SUMS.txt || true
cd ..

# ─── Summary ────────────────────────────────────────────────────
echo ""
echo "Release $VERSION complete:"
echo ""
ls -lh "$RELEASE_DIR/"
