mod app;
mod views;

use app::RshareApp;

fn main() -> eframe::Result<()> {
    // Build a tokio runtime for async operations (poll-promise needs it)
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _guard = rt.enter();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "rshare",
        options,
        Box::new(|cc| Ok(Box::new(RshareApp::new(cc)))),
    )
}
