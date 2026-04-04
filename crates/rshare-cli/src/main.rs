mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rshare", about = "rshare file sharing CLI")]
struct Cli {
    /// Server URL
    #[arg(short, long, default_value = "http://localhost:3000")]
    server: String,

    /// Admin token for protected operations (delete)
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let client = reqwest::Client::new();

    let result = match cli.command {
        Commands::Upload { file } => commands::upload(&client, &cli.server, &file).await,
        Commands::Download { id, output } => {
            commands::download(&client, &cli.server, &id, output.as_deref()).await
        }
        Commands::List => commands::list(&client, &cli.server).await,
        Commands::Delete { id } => {
            commands::delete(&client, &cli.server, &id, cli.token.as_deref()).await
        }
        Commands::Share { id } => commands::share(&client, &cli.server, &id).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}
