mod auth;
mod config;
mod db;
mod handlers;
mod storage;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post};
use clap::Parser;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use config::Config;
use db::Db;
use storage::Storage;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub storage: Arc<Storage>,
    pub admin_token: Option<String>,
    pub default_ttl_hours: u64,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rshare_server=info,tower_http=info".parse().unwrap()),
        )
        .init();

    let cfg = Config::parse();

    std::fs::create_dir_all(&cfg.data_dir).expect("Failed to create data directory");

    let db = Db::open(&cfg.data_dir).expect("Failed to open database");

    // Handle token management commands (run and exit)
    if let Some(spec) = &cfg.create_token {
        handle_create_token(&db, spec);
        return;
    }
    if cfg.list_tokens {
        handle_list_tokens(&db);
        return;
    }
    if let Some(name) = &cfg.revoke_token {
        handle_revoke_token(&db, name);
        return;
    }

    // Auto-migrate admin_token to DB token if set
    if let Some(admin) = &cfg.admin_token
        && db.get_token_by_hash(admin).unwrap_or(None).is_none()
        && !db.has_any_tokens().unwrap_or(false)
    {
        let _ = db.insert_token("admin", admin, &["admin".to_string()]);
        tracing::info!("Auto-created 'admin' API token from --admin-token");
    }

    let storage = Storage::new(&cfg.data_dir)
        .await
        .expect("Failed to init storage");

    let has_tokens = db.has_any_tokens().unwrap_or(false);
    if has_tokens {
        tracing::info!("API tokens are configured — upload requires authentication");
    } else if cfg.admin_token.is_some() {
        tracing::info!("Admin token is set — delete requires authorization");
    } else {
        tracing::warn!("No admin token or API tokens set — all operations are open");
    }

    let state = AppState {
        db: Arc::new(db),
        storage: Arc::new(storage),
        admin_token: cfg.admin_token,
        default_ttl_hours: cfg.default_ttl_hours,
    };

    // Spawn background cleanup task for expired files
    if cfg.default_ttl_hours > 0 {
        let cleanup_state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(15 * 60)).await;
                if let Ok(expired) = cleanup_state.db.list_expired() {
                    for id in &expired {
                        let _ = cleanup_state.storage.delete(*id).await;
                        let _ = cleanup_state.db.delete(*id);
                    }
                    if !expired.is_empty() {
                        tracing::info!("Cleaned up {} expired file(s)", expired.len());
                    }
                }
            }
        });
    }

    let app = Router::new()
        .route("/api/upload", post(handlers::upload))
        .route("/api/files", get(handlers::list_files))
        .route("/api/files/{id}", get(handlers::get_file))
        .route("/api/files/{id}", delete(handlers::delete_file))
        .route("/api/download/{id}", get(handlers::download))
        .route("/api/share/{id}", post(handlers::share_create))
        .route("/share/{token}", get(handlers::share_page))
        .route("/share/{token}/download", get(handlers::share_download))
        .layer(DefaultBodyLimit::max(cfg.max_upload_mb * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cfg.port);
    tracing::info!("rshare server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");
    axum::serve(listener, app).await.expect("Server error");
}

fn handle_create_token(db: &Db, spec: &str) {
    let (name, perms_str) = spec.split_once(':').unwrap_or((spec, "upload,download"));
    let permissions: Vec<String> = perms_str.split(',').map(|s| s.trim().to_string()).collect();
    let raw_token = uuid::Uuid::new_v4().to_string().replace('-', "");
    match db.insert_token(name, &raw_token, &permissions) {
        Ok(()) => {
            println!(
                "Created token '{name}' with permissions: {}",
                permissions.join(", ")
            );
            println!("Token: {raw_token}");
            println!("(save this — it cannot be retrieved later)");
        }
        Err(e) => {
            eprintln!("Failed to create token: {e}");
            std::process::exit(1);
        }
    }
}

fn handle_list_tokens(db: &Db) {
    match db.list_tokens() {
        Ok(tokens) if tokens.is_empty() => println!("No API tokens configured."),
        Ok(tokens) => {
            println!("{:<20} {:<30} Created", "Name", "Permissions");
            println!("{}", "-".repeat(70));
            for t in &tokens {
                println!(
                    "{:<20} {:<30} {}",
                    t.name,
                    t.permissions.join(","),
                    t.created_at.format("%Y-%m-%d %H:%M")
                );
            }
            println!("\n{} token(s)", tokens.len());
        }
        Err(e) => {
            eprintln!("Failed to list tokens: {e}");
            std::process::exit(1);
        }
    }
}

fn handle_revoke_token(db: &Db, name: &str) {
    match db.delete_token(name) {
        Ok(true) => println!("Revoked token '{name}'"),
        Ok(false) => {
            eprintln!("Token '{name}' not found");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to revoke token: {e}");
            std::process::exit(1);
        }
    }
}
