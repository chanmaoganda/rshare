use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Persisted app state: server settings + per-file delete tokens.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppStore {
    pub server_url: String,
    pub admin_token: String,
    /// Map of file UUID → delete token (from uploads made by this client)
    pub delete_tokens: HashMap<String, String>,
}

pub struct Store {
    path: PathBuf,
    data: Mutex<AppStore>,
}

impl Store {
    pub fn load() -> Self {
        let path = store_path();
        let data = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            AppStore::default()
        };
        Self {
            path,
            data: Mutex::new(data),
        }
    }

    pub fn get(&self) -> AppStore {
        self.data.lock().unwrap().clone()
    }

    pub fn set_server(&self, url: &str, token: &str) {
        let mut data = self.data.lock().unwrap();
        data.server_url = url.to_string();
        data.admin_token = token.to_string();
        drop(data);
        self.save();
    }

    pub fn add_delete_token(&self, file_id: &str, delete_token: &str) {
        let mut data = self.data.lock().unwrap();
        data.delete_tokens
            .insert(file_id.to_string(), delete_token.to_string());
        drop(data);
        self.save();
    }

    pub fn get_delete_token(&self, file_id: &str) -> Option<String> {
        self.data
            .lock()
            .unwrap()
            .delete_tokens
            .get(file_id)
            .cloned()
    }

    pub fn remove_delete_token(&self, file_id: &str) {
        let mut data = self.data.lock().unwrap();
        data.delete_tokens.remove(file_id);
        drop(data);
        self.save();
    }

    fn save(&self) {
        let data = self.data.lock().unwrap();
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(&*data).unwrap_or_default();
        let _ = std::fs::write(&self.path, json);
    }
}

/// Returns the path to the app's private data directory.
/// On Android this is the app's internal storage (no permissions needed).
/// On desktop this is the platform config directory.
pub fn app_data_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        // App private internal storage — no permissions required.
        // Matches package name in Cargo.toml metadata.
        PathBuf::from("/data/data/com.rshare.app/files")
    }

    #[cfg(not(target_os = "android"))]
    {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rshare")
    }
}

fn store_path() -> PathBuf {
    app_data_dir().join("config.json")
}
