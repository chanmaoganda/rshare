# rshare

A self-hosted file sharing service written in Rust. Upload, download, and share files through an HTTP API, CLI, or cross-platform GUI (desktop + Android).

## Features

- **Multipart file upload** with configurable size limits
- **Resumable downloads** via HTTP Range headers
- **Shareable links** — generate short tokens for public download URLs
- **Delete tokens** — each upload returns a per-file delete token for uploader-side deletion
- **Optional admin token** — protect delete operations with a global admin token
- **SQLite metadata** — lightweight, zero-config database
- **File-on-disk storage** — uploaded files stored as plain files
- **CLI client** — upload, download, list, delete, and share with progress bars
- **Cross-platform GUI** — Slint-based app for desktop (Linux/macOS/Windows) and Android

## Architecture

rshare is a Cargo workspace with four crates:

| Crate | Description |
|-------|-------------|
| `rshare-common` | Shared types (`FileMetadata`, `UploadResponse`, etc.) |
| `rshare-server` | Axum HTTP server with SQLite + file storage |
| `rshare-cli` | Command-line client (clap + reqwest + indicatif) |
| `rshare-app` | Cross-platform GUI (Slint + reqwest + rfd) — desktop and Android |

## Quick Start

### Build

```bash
# Build everything (debug)
cargo build

# Release build → dist/
./build.sh desktop
```

Binaries will be at `dist/rshare-server`, `dist/rshare-cli`, and `dist/rshare-app`.

### Run the server

```bash
# Default: port 3000, data in ./data, 512 MB max upload
rshare-server

# Custom configuration
rshare-server --port 8080 --data-dir /var/rshare --max-upload-mb 1024

# With admin token (required for delete without per-file token)
rshare-server --admin-token mysecret
# or
RSHARE_ADMIN_TOKEN=mysecret rshare-server
```

### Server Options

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--port`, `-p` | — | `3000` | Listen port |
| `--data-dir`, `-d` | — | `./data` | Storage directory |
| `--max-upload-mb` | — | `512` | Max upload size (MB) |
| `--admin-token` | `RSHARE_ADMIN_TOKEN` | *(none)* | Admin token for delete |

## CLI Usage

```bash
# Upload a file
rshare-cli upload myfile.zip
# → Uploaded: myfile.zip (id: <uuid>)
# → Delete token: <token>

# List files
rshare-cli list

# Download a file
rshare-cli download <id>
rshare-cli download <id> --output renamed.zip

# Download resumes automatically if a partial file exists

# Delete a file (using per-file delete token)
rshare-cli -t <delete-token> delete <id>

# Delete a file (using admin token)
rshare-cli -t <admin-token> delete <id>

# Create a share link
rshare-cli share <id>
# → Share link: http://localhost:3000/share/<token>
```

### CLI Options

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--server`, `-s` | — | `http://localhost:3000` | Server URL |
| `--token`, `-t` | `RSHARE_ADMIN_TOKEN` | *(none)* | Auth token (admin or delete) |

## Desktop App

```bash
rshare-app
```

The Slint-based desktop app provides:
- Server URL and token configuration
- File upload via native file picker
- File list with download, delete, and share actions

## Android

Build the Android library (requires Android NDK):

```bash
# Prerequisites
rustup target add aarch64-linux-android
cargo install cargo-ndk
export ANDROID_NDK_HOME=/path/to/ndk

# Build
./build.sh android
```

The `android` feature flag enables the Slint Android backend. The resulting `.so` is used in an Android APK via `cargo-apk` or a Gradle wrapper project.

## Build Script

```bash
./build.sh desktop   # Server + CLI + desktop app → dist/
./build.sh android   # Android .so → dist/
./build.sh server    # Server only → dist/
./build.sh all       # Everything
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/upload` | Upload a file (multipart form) |
| `GET` | `/api/files` | List all files |
| `GET` | `/api/files/{id}` | Get file metadata |
| `DELETE` | `/api/files/{id}` | Delete a file (requires `Authorization: Bearer <token>`) |
| `GET` | `/api/download/{id}` | Download a file (supports `Range` header) |
| `POST` | `/api/share/{id}` | Create a share link |
| `GET` | `/share/{token}` | Download via share link |

## License

MIT
