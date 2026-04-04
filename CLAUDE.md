# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build                          # Build all crates (debug)
cargo build -p rshare-server         # Server only
cargo build -p rshare-app            # Slint GUI (desktop, default features)
cargo clippy --workspace             # Lint
cargo fmt --all -- --check           # Format check

./build.sh desktop                   # Release: server + CLI + app → dist/
./build.sh android                   # Android APK → dist/rshare-app.apk
./build.sh all                       # Both
./release.sh 0.1.0                   # Package release archives → release/
./release.sh 0.1.0 --android         # Include Android APK
```

### Android build requirements

```bash
# Env vars (must be set)
ANDROID_NDK_HOME=/opt/android-ndk
ANDROID_HOME=/opt/android-sdk
# ANDROID_JAR is auto-resolved by build.sh from ANDROID_HOME

# Tools
cargo install cargo-apk
rustup target add aarch64-linux-android

# Build command (build.sh wraps this)
cargo apk build -p rshare-app --lib --no-default-features --features android --release
```

No tests exist yet.

## Running

```bash
cargo run -p rshare-server                              # Default: port 3000
cargo run -p rshare-server -- --port 8080 --admin-token secret
cargo run -p rshare-cli -- upload myfile.zip
cargo run -p rshare-cli -- list
cargo run -p rshare-cli -- download <uuid>
cargo run -p rshare-cli -- -t <token> delete <uuid>
cargo run -p rshare-cli -- share <uuid>
cargo run -p rshare-app                                 # Desktop GUI
adb install dist/rshare-app.apk                         # Android
```

## Architecture

Self-hosted file sharing service. Cargo workspace with 4 crates:

- **rshare-common** — Shared serde types (`FileMetadata`, `UploadResponse`, `FileListResponse`, `ErrorResponse`). All other crates depend on this.
- **rshare-server** — Axum HTTP server. `AppState` holds `Arc<Db>` + `Arc<Storage>` + optional admin token. Routes in `main.rs`, handlers in `handlers.rs`.
- **rshare-cli** — clap CLI client. Subcommands in `commands.rs`. Resumable downloads with HTTP Range. Progress bars via indicatif.
- **rshare-app** — Cross-platform Slint GUI (desktop + Android).

### rshare-app internals

Dual-target crate: `cdylib` for Android, `rlib` + binary for desktop.

- `lib.rs` — `run_app()` shared entry point + `android_main()` (cfg'd for Android). Wires all Slint callbacks to async handlers via `tokio::spawn` + `slint::invoke_from_event_loop`.
- `desktop.rs` — Desktop binary entry: creates tokio runtime, calls `run_app()`.
- `api.rs` — reqwest HTTP client wrapping all server REST endpoints.
- `models.rs` — Converts `rshare-common` types to Slint `FileEntry` structs.
- `store.rs` — JSON persistence for server URL, admin token, and per-file delete tokens. Stored at `~/.config/rshare/config.json` (desktop) or `/data/data/com.rshare.app/files/config.json` (Android). `app_data_dir()` is the canonical path function.
- `ui/*.slint` — Declarative UI. Material style (`build.rs`). Responsive via `compact` property (true on Android, false on desktop). Custom components: `PrimaryButton`, `GhostButton`, `Badge` in `style.slint`.

Feature flags: `desktop` (default, enables `rfd` file dialogs), `android` (enables Slint Android backend). Build Android with `--no-default-features --features android`.

### Server internals

- **Storage** (`storage.rs`): Files at `{data_dir}/files/{uuid}`.
- **Db** (`db.rs`): SQLite via rusqlite, `Mutex<Connection>`. Single `files` table with auto-migration for `delete_token` column.
- **Handlers** (`handlers.rs`): Multipart upload, list, metadata, download (HTTP Range), delete, share link create/download. `serve_range()` handles partial content.
- **Config** (`config.rs`): clap-derived. `--admin-token` reads `RSHARE_ADMIN_TOKEN` env var.

### Auth model

Delete requires one of: (1) per-file delete token (returned on upload, stored locally by rshare-app), or (2) global admin token. Both sent as `Authorization: Bearer <token>`. The app tries the stored delete token first, then falls back to admin token.

### Android-specific notes

- `use_cleartext_traffic = true` in manifest — needed for LAN HTTP connections.
- Uses `rustls-tls` instead of OpenSSL (can't cross-compile OpenSSL for Android).
- No `rfd` on Android — file picking reads from app-private `uploads/` dir, downloads go to `downloads/` dir.
- Signing uses `debug.keystore` at repo root (gitignored). Generate with `keytool`.
- Android SDK platform symlink may be needed: `ln -sf android-34 android-30` in `$ANDROID_HOME/platforms/`.

## Key Dependencies

Server: axum, rusqlite (bundled), tower-http, tokio, clap
CLI: clap, reqwest, indicatif, anyhow
App: slint (material style), reqwest (rustls-tls), rfd (desktop only), tokio, dirs, serde_json
All: Rust edition 2024
