pub mod api;
pub mod models;

slint::include_modules!();

use api::Api;
use slint::{ModelRc, SharedString, VecModel};
use std::sync::Arc;

/// Create the Slint UI, wire all callbacks, and run the event loop.
/// A tokio runtime must be active (entered) before calling this.
pub fn run_app() {
    let app = App::new().unwrap();
    let api = Arc::new(Api::new());

    setup_connect(&app, &api);
    setup_refresh(&app, &api);
    setup_upload(&app, &api);
    setup_download(&app, &api);
    setup_delete(&app, &api);
    setup_share(&app, &api);

    app.run().unwrap();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: slint::android::AndroidApp) {
    slint::android::init(android_app).unwrap();

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _guard = rt.enter();

    run_app();
}

fn setup_connect(app: &App, api: &Arc<Api>) {
    let weak = app.as_weak();
    let api = api.clone();
    app.on_connect(move || {
        let weak = weak.clone();
        let api = api.clone();
        let server = weak.unwrap().get_server_url().to_string();
        tokio::spawn(async move {
            let result = api.test_connection(&server).await;
            slint::invoke_from_event_loop(move || {
                let app = weak.unwrap();
                match result {
                    Ok(()) => {
                        app.set_connected(true);
                        app.set_settings_status(SharedString::from("Connected"));
                        app.invoke_refresh();
                    }
                    Err(e) => {
                        app.set_connected(false);
                        app.set_settings_status(SharedString::from(format!("Error: {e}")));
                    }
                }
            })
            .unwrap();
        });
    });
}

fn setup_refresh(app: &App, api: &Arc<Api>) {
    let weak = app.as_weak();
    let api = api.clone();
    app.on_refresh(move || {
        let weak = weak.clone();
        let api = api.clone();
        let server = weak.unwrap().get_server_url().to_string();
        slint::invoke_from_event_loop({
            let weak = weak.clone();
            move || weak.unwrap().set_list_loading(true)
        })
        .unwrap();

        tokio::spawn(async move {
            let result = api.list_files(&server).await;
            slint::invoke_from_event_loop(move || {
                let app = weak.unwrap();
                app.set_list_loading(false);
                match result {
                    Ok(files) => {
                        let entries: Vec<FileEntry> = files
                            .iter()
                            .map(|f| {
                                let (id, name, size, date) = models::file_to_entry(f);
                                FileEntry {
                                    id,
                                    name,
                                    size,
                                    uploaded_at: date,
                                }
                            })
                            .collect();
                        app.set_files(ModelRc::new(VecModel::from(entries)));
                        app.set_list_status(SharedString::default());
                        app.set_list_status_is_error(false);
                    }
                    Err(e) => {
                        app.set_list_status(SharedString::from(format!("Error: {e}")));
                        app.set_list_status_is_error(true);
                    }
                }
            })
            .unwrap();
        });
    });
}

fn setup_upload(app: &App, api: &Arc<Api>) {
    let weak = app.as_weak();
    let api = api.clone();
    app.on_pick_and_upload(move || {
        let weak = weak.clone();
        let api = api.clone();
        let server = weak.unwrap().get_server_url().to_string();

        tokio::spawn(async move {
            let file = match pick_file().await {
                Some(f) => f,
                None => return,
            };

            slint::invoke_from_event_loop({
                let weak = weak.clone();
                move || {
                    weak.unwrap().set_uploading(true);
                    weak.unwrap().set_upload_result(SharedString::default());
                }
            })
            .unwrap();

            let result = api.upload(&server, &file.0, file.1).await;

            slint::invoke_from_event_loop(move || {
                let app = weak.unwrap();
                app.set_uploading(false);
                match result {
                    Ok(info) => {
                        app.set_upload_result(SharedString::from(format!(
                            "Uploaded: {} (delete token: {})",
                            info.name, info.delete_token
                        )));
                        app.set_upload_is_error(false);
                        app.invoke_refresh();
                    }
                    Err(e) => {
                        app.set_upload_result(SharedString::from(format!("Error: {e}")));
                        app.set_upload_is_error(true);
                    }
                }
            })
            .unwrap();
        });
    });
}

fn setup_download(app: &App, api: &Arc<Api>) {
    let weak = app.as_weak();
    let api = api.clone();
    app.on_download(move |id| {
        let weak = weak.clone();
        let api = api.clone();
        let server = weak.unwrap().get_server_url().to_string();
        let id = id.to_string();

        tokio::spawn(async move {
            let (filename, data) = match api.download(&server, &id).await {
                Ok(r) => r,
                Err(e) => {
                    slint::invoke_from_event_loop(move || {
                        let app = weak.unwrap();
                        app.set_list_status(SharedString::from(format!("Download error: {e}")));
                        app.set_list_status_is_error(true);
                    })
                    .unwrap();
                    return;
                }
            };

            let save_path = match save_file(&filename).await {
                Some(p) => p,
                None => return,
            };

            let write_result = tokio::fs::write(&save_path, &data).await;

            slint::invoke_from_event_loop(move || {
                let app = weak.unwrap();
                match write_result {
                    Ok(()) => {
                        app.set_list_status(SharedString::from(format!("Saved to: {save_path}")));
                        app.set_list_status_is_error(false);
                    }
                    Err(e) => {
                        app.set_list_status(SharedString::from(format!("Save error: {e}")));
                        app.set_list_status_is_error(true);
                    }
                }
            })
            .unwrap();
        });
    });
}

fn setup_delete(app: &App, api: &Arc<Api>) {
    let weak = app.as_weak();
    let api = api.clone();
    app.on_delete_file(move |id| {
        let weak = weak.clone();
        let api = api.clone();
        let app_ref = weak.unwrap();
        let server = app_ref.get_server_url().to_string();
        let token = app_ref.get_admin_token().to_string();
        let id = id.to_string();

        tokio::spawn(async move {
            let token_ref = if token.is_empty() {
                None
            } else {
                Some(token.as_str())
            };
            let result = api.delete(&server, &id, token_ref).await;

            slint::invoke_from_event_loop(move || {
                let app = weak.unwrap();
                match result {
                    Ok(()) => {
                        app.set_list_status(SharedString::from("Deleted"));
                        app.set_list_status_is_error(false);
                        app.invoke_refresh();
                    }
                    Err(e) => {
                        app.set_list_status(SharedString::from(format!("Delete error: {e}")));
                        app.set_list_status_is_error(true);
                    }
                }
            })
            .unwrap();
        });
    });
}

fn setup_share(app: &App, api: &Arc<Api>) {
    let weak = app.as_weak();
    let api = api.clone();
    app.on_share(move |id| {
        let weak = weak.clone();
        let api = api.clone();
        let server = weak.unwrap().get_server_url().to_string();
        let id = id.to_string();

        tokio::spawn(async move {
            let result = api.share(&server, &id).await;

            slint::invoke_from_event_loop(move || {
                let app = weak.unwrap();
                match result {
                    Ok(url) => {
                        app.set_list_status(SharedString::from(format!("Share link: {url}")));
                        app.set_list_status_is_error(false);
                    }
                    Err(e) => {
                        app.set_list_status(SharedString::from(format!("Share error: {e}")));
                        app.set_list_status_is_error(true);
                    }
                }
            })
            .unwrap();
        });
    });
}

// ── File picking / saving (platform-specific) ───────────────────

/// Pick a file. Returns (filename, bytes) or None if cancelled.
#[cfg(feature = "desktop")]
async fn pick_file() -> Option<(String, Vec<u8>)> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Select file to upload")
        .pick_file()
        .await?;
    let name = handle.file_name();
    let data = handle.read().await;
    Some((name, data))
}

/// Pick a file on Android — read from /sdcard/Download by listing it.
/// For a real app you'd want JNI to the SAF picker, but this works without JNI.
#[cfg(not(feature = "desktop"))]
async fn pick_file() -> Option<(String, Vec<u8>)> {
    // On Android without JNI, we can't open a native file picker.
    // As a fallback, read the most recently modified file from Download.
    let download_dir = android_download_dir();
    let mut entries = tokio::fs::read_dir(&download_dir).await.ok()?;
    let mut newest: Option<(String, std::time::SystemTime)> = None;

    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(meta) = entry.metadata().await {
            if meta.is_file() {
                let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                let dominated = newest.as_ref().is_none_or(|(_, t)| modified > *t);
                if dominated {
                    newest = Some((
                        entry.path().to_string_lossy().to_string(),
                        modified,
                    ));
                }
            }
        }
    }

    let path = newest?.0;
    let name = std::path::Path::new(&path)
        .file_name()?
        .to_string_lossy()
        .to_string();
    let data = tokio::fs::read(&path).await.ok()?;
    Some((name, data))
}

/// Save dialog on desktop via rfd.
#[cfg(feature = "desktop")]
async fn save_file(suggested_name: &str) -> Option<String> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Save downloaded file")
        .set_file_name(suggested_name)
        .save_file()
        .await?;
    Some(handle.path().to_string_lossy().to_string())
}

/// Save on Android — write to Download directory.
#[cfg(not(feature = "desktop"))]
async fn save_file(suggested_name: &str) -> Option<String> {
    let dir = android_download_dir();
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let path = format!("{dir}/{suggested_name}");
    Some(path)
}

#[cfg(not(feature = "desktop"))]
fn android_download_dir() -> String {
    // Android app-specific external storage, or fall back to /sdcard/Download
    std::env::var("EXTERNAL_STORAGE")
        .map(|s| format!("{s}/Download"))
        .unwrap_or_else(|_| "/sdcard/Download".to_string())
}
