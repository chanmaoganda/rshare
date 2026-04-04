use eframe::egui;
use poll_promise::Promise;
use rshare_common::{FileListResponse, FileMetadata};

#[allow(dead_code)]
pub enum FileAction {
    Download(FileMetadata),
    Delete(FileMetadata),
    Share(FileMetadata),
}

pub struct FileListView {
    files: Vec<FileMetadata>,
    refresh_promise: Option<Promise<Result<Vec<FileMetadata>, String>>>,
    delete_promise: Option<Promise<Result<(), String>>>,
    share_promise: Option<Promise<Result<String, String>>>,
    share_result: Option<Result<String, String>>,
    error: Option<String>,
}

impl FileListView {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            refresh_promise: None,
            delete_promise: None,
            share_promise: None,
            share_result: None,
            error: None,
        }
    }

    pub fn refresh(&mut self, server: &str) {
        let server = server.to_string();
        self.refresh_promise = Some(Promise::spawn_async(async move {
            let client = reqwest::Client::new();
            let resp = client
                .get(format!("{server}/api/files"))
                .send()
                .await
                .map_err(|e| format!("Connection error: {e}"))?;

            if !resp.status().is_success() {
                return Err("Failed to list files".to_string());
            }

            let list: FileListResponse = resp
                .json()
                .await
                .map_err(|e| format!("Parse error: {e}"))?;
            Ok(list.files)
        }));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, server: &str) -> Option<FileAction> {
        let mut action = None;

        // Check refresh completion
        if let Some(promise) = &self.refresh_promise {
            if let Some(result) = promise.ready() {
                match result {
                    Ok(files) => {
                        self.files = files.clone();
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e.clone()),
                }
                self.refresh_promise = None;
            }
        }

        // Check delete completion
        if let Some(promise) = &self.delete_promise {
            if promise.ready().is_some() {
                self.delete_promise = None;
                self.refresh(server);
            }
        }

        // Check share completion
        if let Some(promise) = &self.share_promise {
            if let Some(result) = promise.ready() {
                self.share_result = Some(result.clone());
                self.share_promise = None;
            }
        }

        ui.horizontal(|ui| {
            ui.heading("Files");
            if ui.button("⟳ Refresh").clicked() {
                self.refresh(server);
            }
        });
        ui.separator();

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::RED, err);
        }

        if let Some(result) = &self.share_result {
            match result {
                Ok(url) => {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::GREEN, format!("Share link: {url}"));
                        if ui.button("📋 Copy").clicked() {
                            ui.ctx().copy_text(url.clone());
                        }
                    });
                }
                Err(e) => {
                    ui.colored_label(egui::Color32::RED, format!("Share error: {e}"));
                }
            }
        }

        let is_refreshing = self.refresh_promise.is_some();
        if is_refreshing {
            ui.spinner();
        }

        if self.files.is_empty() && !is_refreshing {
            ui.label("No files. Upload something!");
            return None;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("file_list_grid")
                .striped(true)
                .min_col_width(60.0)
                .show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("Size");
                    ui.strong("Uploaded");
                    ui.strong("Actions");
                    ui.end_row();

                    for file in &self.files {
                        ui.label(&file.name);
                        ui.label(humanize_bytes(file.size));
                        ui.label(file.uploaded_at.format("%Y-%m-%d %H:%M").to_string());
                        ui.horizontal(|ui| {
                            if ui.button("⬇ Download").clicked() {
                                action = Some(FileAction::Download(file.clone()));
                            }
                            if ui.button("🔗 Share").clicked() {
                                let server = server.to_string();
                                let id = file.id;
                                self.share_promise =
                                    Some(Promise::spawn_async(async move {
                                        let client = reqwest::Client::new();
                                        let resp = client
                                            .post(format!("{server}/api/share/{id}"))
                                            .send()
                                            .await
                                            .map_err(|e| e.to_string())?;
                                        let body: serde_json::Value =
                                            resp.json().await.map_err(|e| e.to_string())?;
                                        let url = body["share_url"]
                                            .as_str()
                                            .unwrap_or("?")
                                            .to_string();
                                        Ok(format!("{server}{url}"))
                                    }));
                            }
                            if ui.button("🗑 Delete").clicked() {
                                let server = server.to_string();
                                let id = file.id;
                                self.delete_promise =
                                    Some(Promise::spawn_async(async move {
                                        let client = reqwest::Client::new();
                                        let resp = client
                                            .delete(format!("{server}/api/files/{id}"))
                                            .send()
                                            .await
                                            .map_err(|e| e.to_string())?;
                                        if resp.status().is_success() {
                                            Ok(())
                                        } else {
                                            Err("Delete failed".to_string())
                                        }
                                    }));
                            }
                        });
                        ui.end_row();
                    }
                });
        });

        action
    }
}

fn humanize_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    for unit in UNITS {
        if size < 1024.0 {
            return format!("{size:.1} {unit}");
        }
        size /= 1024.0;
    }
    format!("{size:.1} PB")
}
