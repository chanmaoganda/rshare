use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(name = "rshare-server", about = "rshare file sharing server")]
pub struct Config {
    /// Port to listen on
    #[arg(short, long, default_value = "3000")]
    pub port: u16,

    /// Directory to store uploaded files
    #[arg(short, long, default_value = "./data")]
    pub data_dir: PathBuf,

    /// Maximum upload size in megabytes
    #[arg(long, default_value = "512")]
    pub max_upload_mb: usize,

    /// Admin token required for delete operations (if unset, deletes are open)
    #[arg(long, env = "RSHARE_ADMIN_TOKEN")]
    pub admin_token: Option<String>,

    /// Create an API token: NAME:PERM1,PERM2 (e.g., "myapp:upload,download")
    #[arg(long)]
    pub create_token: Option<String>,

    /// List all API tokens
    #[arg(long)]
    pub list_tokens: bool,

    /// Revoke an API token by name
    #[arg(long)]
    pub revoke_token: Option<String>,

    /// Default TTL for uploaded files in hours (0 = no expiry)
    #[arg(long, default_value = "0")]
    pub default_ttl_hours: u64,

    /// Rate limit: max uploads per minute per IP
    #[arg(long, default_value = "10")]
    pub rate_limit_per_minute: u32,

    /// Max concurrent uploads
    #[arg(long, default_value = "4")]
    pub max_concurrent_uploads: usize,
}
