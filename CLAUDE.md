# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build                          # Build all crates (debug)
cargo build --release                # Build all crates (release)
cargo build -p rshare-server         # Build only the server
cargo build -p rshare-cli            # Build only the CLI
cargo build -p rshare-gui            # Build only the GUI
cargo clippy --workspace             # Lint all crates
cargo fmt --all -- --check           # Check formatting
```

No tests exist yet. The project has no test infrastructure.

## Running

```bash
# Server (default: port 3000, data in ./data, 512MB max upload)
cargo run -p rshare-server
cargo run -p rshare-server -- --port 8080 --admin-token secret

# CLI
cargo run -p rshare-cli -- upload myfile.zip
cargo run -p rshare-cli -- list
cargo run -p rshare-cli -- download <uuid>
cargo run -p rshare-cli -- -t <token> delete <uuid>
cargo run -p rshare-cli -- share <uuid>

# GUI
cargo run -p rshare-gui
```

## Architecture

Self-hosted file sharing service. Cargo workspace with 4 crates:

- **rshare-common** — Shared serde types (`FileMetadata`, `UploadResponse`, `FileListResponse`, `ErrorResponse`). All crates depend on this.
- **rshare-server** — Axum HTTP server. `AppState` holds `Arc<Db>` + `Arc<Storage>` + optional admin token. Routes defined in `main.rs`, handlers in `handlers.rs`.
- **rshare-cli** — clap-based CLI client using reqwest. Subcommands dispatched from `main.rs` to functions in `commands.rs`.
- **rshare-gui** — eframe/egui desktop app. `RshareApp` in `app.rs` composes three views (`upload`, `file_list`, `download`) from `views/` module. Runs a tokio runtime for async HTTP via poll-promise.

### Server internals

- **Storage** (`storage.rs`): Files stored as `{data_dir}/files/{uuid}` on disk.
- **Db** (`db.rs`): SQLite via rusqlite with `Mutex<Connection>`. Single `files` table. Auto-migrates `delete_token` column.
- **Handlers** (`handlers.rs`): Multipart upload, list, get metadata, download with HTTP Range support, delete (admin token or per-file delete token), share link creation/download.
- **Config** (`config.rs`): clap-derived config. `--admin-token` also reads `RSHARE_ADMIN_TOKEN` env var.

### Auth model

Two token types for delete: (1) per-file delete token returned on upload, (2) optional global admin token. Both sent as `Authorization: Bearer <token>`. If neither matches, delete is rejected.

## Key Dependencies

Server: axum, rusqlite (bundled), tower-http, tokio
CLI: clap, reqwest, indicatif (progress bars), anyhow
GUI: eframe/egui, reqwest, rfd (file dialogs), poll-promise
All use Rust edition 2024.
