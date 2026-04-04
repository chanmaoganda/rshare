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
    let storage = Storage::new(&cfg.data_dir)
        .await
        .expect("Failed to init storage");

    if cfg.admin_token.is_some() {
        tracing::info!("Admin token is set — delete requires authorization");
    } else {
        tracing::warn!("No admin token set — delete is open to anyone");
    }

    let state = AppState {
        db: Arc::new(db),
        storage: Arc::new(storage),
        admin_token: cfg.admin_token,
    };

    let app = Router::new()
        .route("/api/upload", post(handlers::upload))
        .route("/api/files", get(handlers::list_files))
        .route("/api/files/{id}", get(handlers::get_file))
        .route("/api/files/{id}", delete(handlers::delete_file))
        .route("/api/download/{id}", get(handlers::download))
        .route("/api/share/{id}", post(handlers::share_create))
        .route("/share/{token}", get(handlers::share_download))
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
