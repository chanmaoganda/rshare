# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build                          # Build all crates (debug)
cargo build -p rshare-server         # Server only
cargo build -p rshare-app            # Slint GUI (desktop, default features)
cargo clippy --workspace             # Lint
cargo fmt --all -- --check           # Format check

./build.sh desktop                   # Release: server + CLI + app -> dist/
./build.sh android                   # Android APK -> dist/rshare-app.apk
./build.sh all                       # Both
./build-debug.sh                     # Debug Android APK -> dist/rshare-app-debug.apk
./build-debug.sh --install           # Debug APK + install via adb
./release.sh 0.1.0                   # Package release archives -> release/
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
# Server
cargo run -p rshare-server                              # Default: port 3000
cargo run -p rshare-server -- --port 8080 --admin-token secret
cargo run -p rshare-server -- --default-ttl-hours 168   # Files expire after 7 days

# Token management (runs and exits, no server start)
cargo run -p rshare-server -- --create-token myapp:upload,download
cargo run -p rshare-server -- --list-tokens
cargo run -p rshare-server -- --revoke-token myapp

# CLI (reads server URL + token from ~/.config/rshare/config.json if not specified)
cargo run -p rshare-cli -- upload myfile.zip              # Uses saved config
cargo run -p rshare-cli -- -s http://host:port upload f   # Override server
cargo run -p rshare-cli -- -t <token> upload myfile.zip   # Override token
cargo run -p rshare-cli -- list
cargo run -p rshare-cli -- download <uuid>
cargo run -p rshare-cli -- delete <uuid>
cargo run -p rshare-cli -- share <uuid>

# Desktop GUI + Android
cargo run -p rshare-app
adb install dist/rshare-app.apk
```

## Architecture

Self-hosted file sharing service. Cargo workspace with 4 crates:

- **rshare-common** -- Shared serde types (`FileMetadata`, `UploadResponse`, `FileListResponse`, `ErrorResponse`, `ApiToken`). All other crates depend on this.
- **rshare-server** -- Axum HTTP server. `AppState` holds `Arc<Db>` + `Arc<Storage>` + config. Routes in `main.rs`, handlers in `handlers.rs`, auth in `auth.rs`.
- **rshare-cli** -- clap CLI client. Subcommands in `commands.rs`. Resumable downloads with HTTP Range. Progress bars via indicatif. SHA-256 checksum verification on download.
- **rshare-app** -- Cross-platform Slint GUI (desktop + Android).

> **Note:** `crates/rshare-gui/` exists on disk but is **not** in the workspace — it's an unused iced-based predecessor to rshare-app. Ignore it.

### rshare-app internals

Dual-target crate: `cdylib` for Android, `rlib` + binary for desktop.

- `lib.rs` -- `run_app()` shared entry point + `android_main()` (cfg'd for Android). Wires all Slint callbacks to async handlers via `tokio::spawn` + `slint::invoke_from_event_loop`. Auto-connects on startup if saved config exists. Auto-refreshes file list every 3 seconds while connected.
- `desktop.rs` -- Desktop binary entry: creates tokio runtime, calls `run_app()`.
- `api.rs` -- reqwest HTTP client wrapping all server REST endpoints.
- `models.rs` -- Converts `rshare-common` types to Slint `FileEntry` structs.
- `store.rs` -- JSON persistence for server URL, admin token, and per-file delete tokens. Stored at `~/.config/rshare/config.json` (desktop) or runtime `internal_data_path()` on Android. `app_data_dir()` is the canonical path function. On Android, `set_android_data_dir()` must be called before any store access (done in `android_main()`). The CLI also reads this config as fallback for `--server` and `--token`.
- `ui/*.slint` -- Declarative UI. Material style (`build.rs`). Responsive via `compact` property (true on Android, false on desktop). Custom components: `PrimaryButton`, `GhostButton`, `Badge` in `style.slint`.

Feature flags: `desktop` (default, enables `rfd` file dialogs), `android` (enables Slint Android backend). Build Android with `--no-default-features --features android`.

### API endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/upload` | Upload file (multipart form) |
| `GET` | `/api/files` | List files (`?page=&per_page=`) |
| `GET` | `/api/files/{id}` | File metadata |
| `DELETE` | `/api/files/{id}` | Delete file (requires auth token) |
| `GET` | `/api/download/{id}` | Download file (supports `Range` header) |
| `POST` | `/api/share/{id}` | Create share link |
| `GET` | `/share/{token}` | Share page (HTML for browsers, raw file for CLI) |
| `GET` | `/share/{token}/download` | Direct share download |

### Server internals

- **Storage** (`storage.rs`): Files at `{data_dir}/files/{uuid}`. Streaming uploads via `save_stream()` (writes chunks to disk, computes SHA-256 incrementally). Streaming downloads via `open_file()` + `ReaderStream` (no full-file buffering).
- **Db** (`db.rs`): SQLite via rusqlite, `Mutex<Connection>`. Two tables: `files` (file metadata with auto-migration for new columns) and `api_tokens` (hashed API tokens). `parse_file_row()` helper avoids `unwrap()` on corrupt data. Paginated listing via `LIMIT/OFFSET`.
- **Handlers** (`handlers.rs`): Streaming multipart upload, paginated list (`?page=&per_page=`), metadata, streaming download (HTTP Range via seek), delete, share link create, HTML share page, share download. `serve_range_stream()` handles partial content with file seeking.
- **Auth** (`auth.rs`): `AuthContext` axum extractor. If API tokens exist in DB, requires `Authorization: Bearer <token>`. If no tokens configured, passes through (backward compat). Upload requires "upload" permission; delete checks API token OR per-file delete token.
- **Config** (`config.rs`): clap-derived. Server flags: `--admin-token`, `--create-token`, `--list-tokens`, `--revoke-token`, `--default-ttl-hours`, `--rate-limit-per-minute`, `--max-concurrent-uploads`.

### Auth model

Two-layer auth:
1. **API tokens** -- Named tokens stored as SHA-256 hashes in `api_tokens` table. Created via `--create-token NAME:PERMS`. Permissions: `upload`, `download`, `delete`, `admin`. If any tokens exist, upload requires auth. If none exist, all operations are open (backward compat).
2. **Per-file delete tokens** -- Returned on upload, stored locally by rshare-app/CLI. Used for file-specific deletion without admin access.

Legacy `--admin-token` is auto-migrated to a DB token named "admin" with all permissions on first startup.

### File lifecycle

- Upload: streaming to disk + SHA-256 + content-type detection (multipart header or `mime_guess`) + optional TTL (`--default-ttl-hours`)
- Download: streaming from disk with HTTP Range support; expired files return 410 Gone
- Share: `GET /share/{token}` serves HTML download page for browsers (Accept: text/html) or raw file for CLI; `GET /share/{token}/download` always serves file
- Expiration: background tokio task runs every 15 minutes, deletes expired files from storage + DB

### DB schema migrations

New columns are added via `pragma_table_info` checks + `ALTER TABLE ADD COLUMN` in `Db::open()`. All new fields are nullable for backward compat with existing databases. Current columns on `files`: `id`, `name`, `size`, `uploaded_at`, `share_token`, `delete_token`, `content_type`, `sha256`, `expires_at`.

### Android-specific notes

- `use_cleartext_traffic = true` in manifest -- needed for LAN HTTP connections.
- Uses `rustls-tls` instead of OpenSSL (can't cross-compile OpenSSL for Android).
- No `rfd` on Android -- file picking reads from app-private `uploads/` dir, downloads go to `/sdcard/Download/rshare/` (public, user-accessible).
- Android data path is queried at runtime via `AndroidApp::internal_data_path()` -- never hardcode `/data/data/...` paths.
- Debug APK (`build-debug.sh`) allows `adb shell run-as com.rshare.app` for inspecting app data. Release and debug APKs have different signatures -- must `adb uninstall com.rshare.app` when switching between them.
- Signing uses `debug.keystore` at repo root (gitignored). Generate with `keytool`.
- Android SDK platform symlink may be needed: `ln -sf android-34 android-30` in `$ANDROID_HOME/platforms/`.
- Permissions: INTERNET, ACCESS_NETWORK_STATE, WRITE/READ_EXTERNAL_STORAGE, MANAGE_EXTERNAL_STORAGE (for public Downloads access).

## Key Dependencies

Server: axum, rusqlite (bundled), tower-http, tokio, tokio-util, clap, sha2, futures, mime_guess
CLI: clap, reqwest (stream), indicatif, anyhow, sha2, dirs
App: slint (material style), reqwest (rustls-tls), rfd (desktop only), tokio, dirs, serde_json
All: Rust edition 2024
