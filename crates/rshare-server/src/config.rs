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
}
