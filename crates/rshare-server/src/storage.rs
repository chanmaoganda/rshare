use axum::body::Bytes;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

pub struct Storage {
    base_dir: PathBuf,
}

/// Result of streaming a file to disk.
pub struct SaveResult {
    pub size: u64,
    pub sha256: String,
}

impl Storage {
    pub async fn new(base_dir: &Path) -> std::io::Result<Self> {
        let files_dir = base_dir.join("files");
        fs::create_dir_all(&files_dir).await?;
        Ok(Self {
            base_dir: files_dir,
        })
    }

    fn file_path(&self, id: Uuid) -> PathBuf {
        self.base_dir.join(id.to_string())
    }

    /// Stream upload data to disk, computing SHA-256 and size incrementally.
    pub async fn save_stream<S, E>(&self, id: Uuid, mut stream: S) -> std::io::Result<SaveResult>
    where
        S: futures::Stream<Item = Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let path = self.file_path(id);
        let mut file = fs::File::create(&path).await?;
        let mut hasher = Sha256::new();
        let mut size: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(std::io::Error::other)?;
            hasher.update(&chunk);
            size += chunk.len() as u64;
            file.write_all(&chunk).await?;
        }

        file.flush().await?;
        let sha256 = format!("{:x}", hasher.finalize());
        Ok(SaveResult { size, sha256 })
    }

    /// Open a file for streaming download, returning the handle and total size.
    pub async fn open_file(&self, id: Uuid) -> std::io::Result<(fs::File, u64)> {
        let path = self.file_path(id);
        let file = fs::File::open(&path).await?;
        let metadata = file.metadata().await?;
        Ok((file, metadata.len()))
    }

    pub async fn delete(&self, id: Uuid) -> std::io::Result<()> {
        let path = self.file_path(id);
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }
}
