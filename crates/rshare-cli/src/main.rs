mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rshare", about = "rshare file sharing CLI")]
struct Cli {
    /// Server URL (reads from ~/.config/rshare/config.json if not set)
    #[arg(short, long)]
    server: Option<String>,

    /// Admin token for protected operations (reads from config or RSHARE_ADMIN_TOKEN)
    #[arg(short = 't', long, env = "RSHARE_ADMIN_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Upload a file to the server
    Upload {
        /// Path to the file to upload
        file: PathBuf,
    },
    /// Download a file from the server
    Download {
        /// File ID
        id: String,
        /// Output path (defaults to original filename)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List all files on the server
    List,
    /// Delete a file from the server
    Delete {
        /// File ID
        id: String,
    },
    /// Create a share link for a file
    Share {
        /// File ID
        id: String,
    },
}

/// Saved config from rshare-app (shared between CLI and GUI).
#[derive(serde::Deserialize, Default)]
struct SavedConfig {
    #[serde(default)]
    server_url: String,
    #[serde(default)]
    admin_token: String,
}

fn load_saved_config() -> SavedConfig {
    let path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rshare")
        .join("config.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let saved = load_saved_config();

    let server = cli
        .server
        .unwrap_or_else(|| {
            if !saved.server_url.is_empty() {
                saved.server_url.clone()
            } else {
                "http://localhost:3000".to_string()
            }
        });

    let token = cli.token.or_else(|| {
        if saved.admin_token.is_empty() {
            None
        } else {
            Some(saved.admin_token.clone())
        }
    });

    let client = reqwest::Client::new();

    let result = match cli.command {
        Commands::Upload { file } => {
            commands::upload(&client, &server, &file, token.as_deref()).await
        }
        Commands::Download { id, output } => {
            commands::download(&client, &server, &id, output.as_deref()).await
        }
        Commands::List => commands::list(&client, &server).await,
        Commands::Delete { id } => {
            commands::delete(&client, &server, &id, token.as_deref()).await
        }
        Commands::Share { id } => commands::share(&client, &server, &id).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}
