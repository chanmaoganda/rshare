use poll_promise::Promise;
use rshare_common::FileMetadata;

pub struct DownloadManager {
    promise: Option<Promise<Result<(), String>>>,
    pub last_result: Option<Result<String, String>>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            promise: None,
            last_result: None,
        }
    }

    pub fn start_download(&mut self, server: &str, meta: &FileMetadata) {
        let server = server.to_string();
        let id = meta.id;
        let name = meta.name.clone();

        // Prompt save location
        let save_path = rfd::FileDialog::new()
            .set_file_name(&name)
            .save_file();

        let Some(save_path) = save_path else {
            return;
        };

        self.promise = Some(Promise::spawn_async(async move {
            let client = reqwest::Client::new();
            let resp = client
                .get(format!("{server}/api/download/{id}"))
                .send()
                .await
                .map_err(|e| format!("Download error: {e}"))?;

            if !resp.status().is_success() {
                return Err("Download failed".to_string());
            }

            let data = resp.bytes().await.map_err(|e| format!("Read error: {e}"))?;
            tokio::fs::write(&save_path, &data)
                .await
                .map_err(|e| format!("Save error: {e}"))?;
            Ok(())
        }));
    }

    pub fn is_downloading(&self) -> bool {
        self.promise
            .as_ref()
            .is_some_and(|p| p.ready().is_none())
    }

    pub fn poll(&mut self) {
        if let Some(promise) = &self.promise {
            if let Some(result) = promise.ready() {
                self.last_result = Some(match result {
                    Ok(()) => Ok("Download complete".to_string()),
                    Err(e) => Err(e.clone()),
                });
                self.promise = None;
            }
        }
    }
}
