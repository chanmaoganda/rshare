# rshare

A self-hosted file sharing service written in Rust. Upload, download, and share files through an HTTP API, CLI, or cross-platform GUI (desktop + Android).

## Features

- **Streaming uploads & downloads** — no full-file buffering, handles large files efficiently
- **Resumable downloads** via HTTP Range headers
- **SHA-256 checksums** — verified on download to ensure file integrity
- **Shareable links** — generate short tokens for public download URLs
- **API token system** — named tokens with granular permissions (upload, download, delete, admin)
- **Per-file delete tokens** — each upload returns a token for uploader-side deletion
- **File expiration** — configurable TTL with automatic cleanup
- **Rate limiting** — per-IP upload rate limiting
- **Concurrent upload limits** — configurable max simultaneous uploads
- **Paginated file listing** — `?page=&per_page=` query parameters
- **SQLite metadata** — lightweight, zero-config database with auto-migration
- **File-on-disk storage** — uploaded files stored as plain files
- **CLI client** — upload, download, list, delete, and share with progress bars
- **Cross-platform GUI** — Slint-based app for desktop (Linux/macOS/Windows) and Android

## Architecture

rshare is a Cargo workspace with four crates:

| Crate | Description |
|-------|-------------|
| `rshare-common` | Shared types (`FileMetadata`, `UploadResponse`, `ApiToken`, etc.) |
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

# With file expiration (files expire after 7 days)
rshare-server --default-ttl-hours 168

# With rate limiting
rshare-server --rate-limit-per-minute 5 --max-concurrent-uploads 2
```

### Server Options

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--port`, `-p` | — | `3000` | Listen port |
| `--data-dir`, `-d` | — | `./data` | Storage directory |
| `--max-upload-mb` | — | `512` | Max upload size (MB) |
| `--admin-token` | `RSHARE_ADMIN_TOKEN` | *(none)* | Legacy admin token (auto-migrated to DB token) |
| `--create-token` | — | — | Create API token: `NAME:PERM1,PERM2` |
| `--list-tokens` | — | — | List all API tokens |
| `--revoke-token` | — | — | Revoke token by name |
| `--default-ttl-hours` | — | `0` | File expiration in hours (0 = no expiry) |
| `--rate-limit-per-minute` | — | `10` | Max uploads per minute per IP |
| `--max-concurrent-uploads` | — | `4` | Max concurrent uploads |

### Token Management

API tokens provide granular access control. Available permissions: `upload`, `download`, `delete`, `admin`.

```bash
# Create a token with specific permissions
rshare-server --create-token myapp:upload,download

# Create an admin token (all permissions)
rshare-server --create-token ci:admin

# List all tokens
rshare-server --list-tokens

# Revoke a token
rshare-server --revoke-token myapp
```

Token management commands run and exit immediately — they don't start the server.

### Auth Model

rshare uses two-layer authentication:

1. **API tokens** — Named tokens with permissions, stored as SHA-256 hashes in the database. If any tokens exist, upload requires a valid token. If no tokens are configured, all operations are open (backward compatible).
2. **Per-file delete tokens** — Returned on each upload. Allows the uploader to delete their own file without admin access.

Legacy `--admin-token` values are automatically migrated to a DB token named "admin" with all permissions on first startup.

## CLI Usage

```bash
# Upload a file
rshare-cli upload myfile.zip
# → Uploaded: myfile.zip (id: <uuid>)
# → Delete token: <token>

# Upload with auth token (required when API tokens are configured)
rshare-cli -t <token> upload myfile.zip

# List files
rshare-cli list

# Download a file (resumes automatically if partial file exists)
rshare-cli download <id>
rshare-cli download <id> --output renamed.zip

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
| `--token`, `-t` | `RSHARE_ADMIN_TOKEN` | *(none)* | Auth token (API or per-file delete) |

## Desktop App

```bash
rshare-app
```

The Slint-based desktop app provides:
- Server URL and token configuration
- File upload via native file picker
- File list with download, delete, and share actions

## Android

Build the Android APK (requires Android NDK + SDK):

```bash
# Prerequisites
rustup target add aarch64-linux-android
cargo install cargo-apk
export ANDROID_NDK_HOME=/path/to/ndk
export ANDROID_HOME=/path/to/sdk

# Build
./build.sh android

# Install
adb install dist/rshare-app.apk
```

The `android` feature flag enables the Slint Android backend and uses `rustls-tls` for HTTP.

## Build Script

```bash
./build.sh desktop   # Server + CLI + desktop app → dist/
./build.sh android   # Android APK → dist/
./build.sh all       # Everything

# Release packaging
./release.sh 0.1.0             # Package release archives → release/
./release.sh 0.1.0 --android   # Include Android APK
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/upload` | Upload a file (multipart form) |
| `GET` | `/api/files` | List files (`?page=&per_page=` for pagination) |
| `GET` | `/api/files/{id}` | Get file metadata |
| `DELETE` | `/api/files/{id}` | Delete a file (requires auth token) |
| `GET` | `/api/download/{id}` | Download a file (supports `Range` header) |
| `POST` | `/api/share/{id}` | Create a share link |
| `GET` | `/share/{token}` | Share page (HTML for browsers, raw file for CLI) |
| `GET` | `/share/{token}/download` | Direct share download |

## License

MIT
