use eframe::egui;

use crate::views::download::DownloadManager;
use crate::views::file_list::{FileAction, FileListView};
use crate::views::upload::UploadView;

pub struct RshareApp {
    server_url: String,
    admin_token: String,
    upload_view: UploadView,
    file_list: FileListView,
    download_mgr: DownloadManager,
    initial_refresh: bool,
}

impl RshareApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            server_url: "http://localhost:3000".to_string(),
            admin_token: String::new(),
            upload_view: UploadView::new(),
            file_list: FileListView::new(),
            download_mgr: DownloadManager::new(),
            initial_refresh: false,
        }
    }
}

impl eframe::App for RshareApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.initial_refresh {
            self.file_list.refresh(&self.server_url);
            self.initial_refresh = true;
        }

        // Poll downloads
        self.download_mgr.poll();

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("rshare");
                ui.separator();
                ui.label("Server:");
                ui.text_edit_singleline(&mut self.server_url);
                ui.separator();
                ui.label("Token:");
                ui.add(egui::TextEdit::singleline(&mut self.admin_token).password(true).desired_width(120.0));
                if ui.button("Connect").clicked() {
                    self.file_list.refresh(&self.server_url);
                }
            });
        });

        egui::SidePanel::left("upload_panel")
            .resizable(true)
            .min_width(250.0)
            .show(ctx, |ui| {
                let uploaded = self.upload_view.ui(ui, &self.server_url);
                if uploaded {
                    self.file_list.refresh(&self.server_url);
                }

                ui.separator();

                if self.download_mgr.is_downloading() {
                    ui.spinner();
                    ui.label("Downloading...");
                }
                if let Some(result) = &self.download_mgr.last_result {
                    match result {
                        Ok(msg) => ui.colored_label(egui::Color32::GREEN, msg),
                        Err(e) => ui.colored_label(egui::Color32::RED, e),
                    };
                }
            });

        let token = if self.admin_token.is_empty() {
            None
        } else {
            Some(self.admin_token.clone())
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(action) = self.file_list.ui(ui, &self.server_url, token.as_deref()) {
                match action {
                    FileAction::Download(meta) => {
                        self.download_mgr.start_download(&self.server_url, &meta);
                    }
                    FileAction::Delete(_) | FileAction::Share(_) => {
                        // Handled internally by file_list
                    }
                }
            }
        });

        // Keep repainting while async operations are in flight
        ctx.request_repaint();
    }
}
