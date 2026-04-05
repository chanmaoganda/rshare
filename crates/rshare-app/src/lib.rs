pub mod api;
pub mod models;
pub mod store;

slint::include_modules!();

use api::Api;
use slint::{ModelRc, SharedString, VecModel};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use store::Store;

/// Create the Slint UI, wire all callbacks, and run the event loop.
/// A tokio runtime must be active (entered) before calling this.
pub fn run_app() {
    let app = App::new().unwrap();
    let api = Arc::new(Api::new());
    let store = Arc::new(Store::load());

    // Set compact mode: always on Android, never on desktop
    // (desktop users can resize but the layout is designed for wide screens)
    #[cfg(target_os = "android")]
    app.set_compact(true);

    // Restore saved settings
    let saved = store.get();
    if !saved.server_url.is_empty() {
        app.set_server_url(SharedString::from(&saved.server_url));
    }
    if !saved.admin_token.is_empty() {
        app.set_admin_token(SharedString::from(&saved.admin_token));
    }

    setup_connect(&app, &api, &store);
    setup_refresh(&app, &api);
    setup_upload(&app, &api, &store);
    setup_download(&app, &api);
    setup_delete(&app, &api, &store);
    setup_share(&app, &api);

    // Auto-connect if saved server URL exists
    if !saved.server_url.is_empty() {
        app.invoke_connect();
    }

    app.run().unwrap();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: slint::android::AndroidApp) {
    let data_path = android_app
        .internal_data_path()
        .expect("internal_data_path unavailable");
    store::set_android_data_dir(data_path);

    slint::android::init(android_app).unwrap();

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _guard = rt.enter();

    run_app();
}

fn setup_connect(app: &App, api: &Arc<Api>, store: &Arc<Store>) {
    let weak = app.as_weak();
    let api = api.clone();
    let store = store.clone();
    app.on_connect(move || {
        let weak = weak.clone();
        let api = api.clone();
        let store = store.clone();
        let app_ref = weak.unwrap();
        let server = app_ref.get_server_url().to_string();
        let token = app_ref.get_admin_token().to_string();
        tokio::spawn(async move {
            let result = api.test_connection(&server).await;
            let server2 = server.clone();
            let token2 = token.clone();
            slint::invoke_from_event_loop(move || {
                let app = weak.unwrap();
                match result {
                    Ok(()) => {
                        store.set_server(&server2, &token2);
                        app.set_connected(true);
                        app.set_settings_status(SharedString::from("Connected"));
                        app.invoke_refresh();

                        // Start periodic refresh every 5 seconds
                        let weak2 = app.as_weak();
                        let connected_flag = Arc::new(AtomicBool::new(true));
                        let flag_clone = connected_flag.clone();
                        // Store flag so disconnect can stop the timer
                        // (connected_flag stays alive via the spawned task)
                        tokio::spawn(async move {
                            loop {
                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                if !flag_clone.load(Ordering::Relaxed) {
                                    break;
                                }
                                let weak2 = weak2.clone();
                                let flag = flag_clone.clone();
                                if slint::invoke_from_event_loop(move || {
                                    let app = weak2.unwrap();
                                    if app.get_connected() {
                                        app.invoke_refresh();
                                    } else {
                                        flag.store(false, Ordering::Relaxed);
                                    }
                                })
                                .is_err()
                                {
                                    break;
                                }
                            }
                        });
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
                                let (id, name, size, date, content_type, sha256, expires_at) =
                                    models::file_to_entry(f);
                                FileEntry {
                                    id,
                                    name,
                                    size,
                                    uploaded_at: date,
                                    content_type,
                                    sha256,
                                    expires_at,
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

fn setup_upload(app: &App, api: &Arc<Api>, store: &Arc<Store>) {
    let weak = app.as_weak();
    let api = api.clone();
    let store = store.clone();
    app.on_pick_and_upload(move || {
        let weak = weak.clone();
        let api = api.clone();
        let store = store.clone();
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

            let admin_token = store.get().admin_token;
            let token = if admin_token.is_empty() {
                None
            } else {
                Some(admin_token)
            };
            let result = api.upload(&server, &file.0, file.1, token.as_deref()).await;

            slint::invoke_from_event_loop(move || {
                let app = weak.unwrap();
                app.set_uploading(false);
                match result {
                    Ok(info) => {
                        // Save the delete token locally
                        store.add_delete_token(&info.id.to_string(), &info.delete_token);
                        app.set_upload_result(SharedString::from(format!(
                            "Uploaded: {}",
                            info.name
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

fn setup_delete(app: &App, api: &Arc<Api>, store: &Arc<Store>) {
    let weak = app.as_weak();
    let api = api.clone();
    let store = store.clone();
    app.on_delete_file(move |id| {
        let weak = weak.clone();
        let api = api.clone();
        let store = store.clone();
        let app_ref = weak.unwrap();
        let server = app_ref.get_server_url().to_string();
        let admin_token = app_ref.get_admin_token().to_string();
        let id = id.to_string();

        // Try stored delete token first, then admin token
        let token = store.get_delete_token(&id).or_else(|| {
            if admin_token.is_empty() {
                None
            } else {
                Some(admin_token)
            }
        });

        tokio::spawn(async move {
            let result = api.delete(&server, &id, token.as_deref()).await;

            slint::invoke_from_event_loop(move || {
                let app = weak.unwrap();
                match result {
                    Ok(()) => {
                        store.remove_delete_token(&id);
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

#[cfg(not(feature = "desktop"))]
async fn pick_file() -> Option<(String, Vec<u8>)> {
    // On Android without JNI, scan the app's uploads directory for the newest file.
    // Users should place files there via adb push or a file manager.
    let upload_dir = store::app_data_dir().join("uploads");
    let _ = tokio::fs::create_dir_all(&upload_dir).await;
    let mut entries = tokio::fs::read_dir(&upload_dir).await.ok()?;
    let mut newest: Option<(String, std::time::SystemTime)> = None;

    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(meta) = entry.metadata().await {
            if meta.is_file() {
                let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                let dominated = newest.as_ref().is_none_or(|(_, t)| modified > *t);
                if dominated {
                    newest = Some((entry.path().to_string_lossy().to_string(), modified));
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

#[cfg(feature = "desktop")]
async fn save_file(suggested_name: &str) -> Option<String> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Save downloaded file")
        .set_file_name(suggested_name)
        .save_file()
        .await?;
    Some(handle.path().to_string_lossy().to_string())
}

#[cfg(not(feature = "desktop"))]
async fn save_file(suggested_name: &str) -> Option<String> {
    // Save to public Downloads folder so users can access files
    let dir = std::path::PathBuf::from("/sdcard/Download/rshare");
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let path = dir.join(suggested_name);
    Some(path.to_string_lossy().to_string())
}
