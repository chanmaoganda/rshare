use rshare_common::FileMetadata;
use slint::SharedString;

/// Convert a `FileMetadata` into the Slint `FileEntry` struct fields.
pub fn file_to_entry(
    f: &FileMetadata,
) -> (
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
) {
    (
        SharedString::from(f.id.to_string()),
        SharedString::from(&f.name),
        SharedString::from(humanize_bytes(f.size)),
        SharedString::from(f.uploaded_at.format("%Y-%m-%d %H:%M").to_string()),
        SharedString::from(f.content_type.as_deref().unwrap_or("")),
        SharedString::from(f.sha256.as_deref().unwrap_or("")),
        SharedString::from(
            f.expires_at
                .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_default(),
        ),
    )
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
