use eframe::egui;
use poll_promise::Promise;
use reqwest::multipart;
use rshare_common::UploadResponse;
use std::path::PathBuf;

pub struct UploadView {
    selected_file: Option<PathBuf>,
    upload_promise: Option<Promise<Result<UploadResponse, String>>>,
    last_result: Option<Result<UploadResponse, String>>,
}

impl UploadView {
    pub fn new() -> Self {
        Self {
            selected_file: None,
            upload_promise: None,
            last_result: None,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, server: &str) -> bool {
        let mut uploaded = false;

        ui.heading("Upload");
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("Choose file...").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.selected_file = Some(path);
                    self.last_result = None;
                }
            }
            if let Some(path) = &self.selected_file {
                ui.label(path.display().to_string());
            } else {
                ui.label("No file selected");
            }
        });

        let is_uploading = self
            .upload_promise
            .as_ref()
            .is_some_and(|p| p.ready().is_none());

        ui.add_enabled_ui(!is_uploading && self.selected_file.is_some(), |ui| {
            if ui.button("Upload").clicked() {
                if let Some(path) = self.selected_file.clone() {
                    let server = server.to_string();
                    self.upload_promise = Some(Promise::spawn_async(async move {
                        do_upload(&server, &path).await
                    }));
                }
            }
        });

        if is_uploading {
            ui.spinner();
            ui.label("Uploading...");
        }

        // Check if promise completed
        let ready = self
            .upload_promise
            .as_ref()
            .and_then(|p| p.ready().cloned());
        if let Some(result) = ready {
            if result.is_ok() {
                uploaded = true;
                self.selected_file = None;
            }
            self.last_result = Some(result);
            self.upload_promise = None;
        }

        if let Some(result) = &self.last_result {
            match result {
                Ok(resp) => {
                    ui.colored_label(egui::Color32::GREEN, format!("Uploaded: {} ({})", resp.name, resp.id));
                }
                Err(e) => {
                    ui.colored_label(egui::Color32::RED, format!("Error: {e}"));
                }
            }
        }

        uploaded
    }
}

async fn do_upload(server: &str, path: &std::path::Path) -> Result<UploadResponse, String> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unnamed".to_string());

    let data = tokio::fs::read(path)
        .await
        .map_err(|e| format!("Read error: {e}"))?;

    let part = multipart::Part::bytes(data)
        .file_name(file_name)
        .mime_str("application/octet-stream")
        .map_err(|e| format!("Mime error: {e}"))?;
    let form = multipart::Form::new().part("file", part);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{server}/api/upload"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Upload error: {e}"))?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Server error: {text}"));
    }

    resp.json::<UploadResponse>()
        .await
        .map_err(|e| format!("Parse error: {e}"))
}
