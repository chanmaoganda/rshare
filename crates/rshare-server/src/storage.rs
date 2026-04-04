use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

pub struct Storage {
    base_dir: PathBuf,
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

    pub async fn save(&self, id: Uuid, data: &[u8]) -> std::io::Result<()> {
        fs::write(self.file_path(id), data).await
    }

    pub async fn read(&self, id: Uuid) -> std::io::Result<Vec<u8>> {
        fs::read(self.file_path(id)).await
    }

    pub async fn delete(&self, id: Uuid) -> std::io::Result<()> {
        let path = self.file_path(id);
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }
}
