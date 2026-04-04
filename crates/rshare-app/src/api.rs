use anyhow::{Context, Result, bail};
use reqwest::Client;
use reqwest::multipart;
use rshare_common::{FileListResponse, FileMetadata, UploadResponse};

pub struct Api {
    client: Client,
}

impl Api {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn test_connection(&self, server: &str) -> Result<()> {
        let resp = self
            .client
            .get(format!("{server}/api/files"))
            .send()
            .await
            .context("Failed to connect to server")?;
        if !resp.status().is_success() {
            bail!("Server returned {}", resp.status());
        }
        Ok(())
    }

    pub async fn list_files(&self, server: &str) -> Result<Vec<FileMetadata>> {
        let resp = self
            .client
            .get(format!("{server}/api/files"))
            .send()
            .await
            .context("Failed to connect to server")?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("List failed: {text}");
        }
        let list: FileListResponse = resp.json().await?;
        Ok(list.files)
    }

    pub async fn upload(
        &self,
        server: &str,
        file_name: &str,
        data: Vec<u8>,
        token: Option<&str>,
    ) -> Result<UploadResponse> {
        let part = multipart::Part::bytes(data)
            .file_name(file_name.to_string())
            .mime_str("application/octet-stream")?;
        let form = multipart::Form::new().part("file", part);

        let mut req = self
            .client
            .post(format!("{server}/api/upload"))
            .multipart(form);
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.context("Failed to connect to server")?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Upload failed: {text}");
        }
        let info: UploadResponse = resp.json().await?;
        Ok(info)
    }

    pub async fn download(&self, server: &str, id: &str) -> Result<(String, Vec<u8>)> {
        let meta: FileMetadata = self
            .client
            .get(format!("{server}/api/files/{id}"))
            .send()
            .await
            .context("Failed to connect")?
            .json()
            .await?;

        let resp = self
            .client
            .get(format!("{server}/api/download/{id}"))
            .send()
            .await
            .context("Failed to download")?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Download failed: {text}");
        }
        let bytes = resp.bytes().await?.to_vec();
        Ok((meta.name, bytes))
    }

    pub async fn delete(&self, server: &str, id: &str, token: Option<&str>) -> Result<()> {
        let mut req = self.client.delete(format!("{server}/api/files/{id}"));
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.context("Failed to connect")?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Delete failed: {text}");
        }
        Ok(())
    }

    pub async fn share(&self, server: &str, id: &str) -> Result<String> {
        let resp = self
            .client
            .post(format!("{server}/api/share/{id}"))
            .send()
            .await
            .context("Failed to connect")?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("Share failed: {text}");
        }
        let body: serde_json::Value = resp.json().await?;
        let share_url = body["share_url"].as_str().unwrap_or("?").to_string();
        Ok(format!("{server}{share_url}"))
    }
}
