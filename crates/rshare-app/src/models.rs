use rshare_common::FileMetadata;
use slint::{Color, SharedString};

use crate::FileEntry;

/// Convert a `FileMetadata` into the Slint `FileEntry` struct.
pub fn file_to_entry(f: &FileMetadata) -> FileEntry {
    let content_type = f.content_type.as_deref().unwrap_or("");
    let (icon, icon_color, icon_bg) = file_type_icon(content_type);

    FileEntry {
        id: SharedString::from(f.id.to_string()),
        name: SharedString::from(&f.name),
        size: SharedString::from(humanize_bytes(f.size)),
        uploaded_at: SharedString::from(f.uploaded_at.format("%Y-%m-%d %H:%M").to_string()),
        content_type: SharedString::from(content_type),
        sha256: SharedString::from(f.sha256.as_deref().unwrap_or("")),
        expires_at: SharedString::from(
            f.expires_at
                .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_default(),
        ),
        file_icon: SharedString::from(icon),
        icon_color,
        icon_bg,
    }
}

fn file_type_icon(content_type: &str) -> (&'static str, Color, Color) {
    if content_type.starts_with("image/") {
        (
            "IMG",
            Color::from_rgb_u8(0xE9, 0x1E, 0x63),
            Color::from_rgb_u8(0xFC, 0xE4, 0xEC),
        )
    } else if content_type.starts_with("video/") {
        (
            "VID",
            Color::from_rgb_u8(0x9C, 0x27, 0xB0),
            Color::from_rgb_u8(0xF3, 0xE5, 0xF5),
        )
    } else if content_type.starts_with("audio/") {
        (
            "AUD",
            Color::from_rgb_u8(0xFF, 0x98, 0x00),
            Color::from_rgb_u8(0xFF, 0xF3, 0xE0),
        )
    } else if content_type.starts_with("text/") {
        (
            "TXT",
            Color::from_rgb_u8(0x60, 0x7D, 0x8B),
            Color::from_rgb_u8(0xEC, 0xEF, 0xF1),
        )
    } else if content_type == "application/pdf" {
        (
            "PDF",
            Color::from_rgb_u8(0xD3, 0x2F, 0x2F),
            Color::from_rgb_u8(0xFF, 0xEB, 0xEE),
        )
    } else if content_type.contains("zip")
        || content_type.contains("tar")
        || content_type.contains("compress")
        || content_type.contains("archive")
    {
        (
            "ZIP",
            Color::from_rgb_u8(0x79, 0x55, 0x48),
            Color::from_rgb_u8(0xEF, 0xEB, 0xE9),
        )
    } else {
        (
            "FILE",
            Color::from_rgb_u8(0x19, 0x76, 0xD2),
            Color::from_rgb_u8(0xE3, 0xF2, 0xFD),
        )
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
