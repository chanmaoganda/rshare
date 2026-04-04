use anyhow::{Context, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use reqwest::multipart;
use rshare_common::{FileListResponse, FileMetadata, UploadResponse};
use std::path::Path;
use tokio::fs;

pub async fn upload(client: &Client, server: &str, file_path: &Path) -> Result<()> {
    let file_name = file_path
        .file_name()
        .context("Invalid file path")?
        .to_string_lossy()
        .to_string();

    let data = fs::read(file_path)
        .await
        .context("Failed to read file")?;

    let size = data.len() as u64;
    let pb = ProgressBar::new(size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
            .progress_chars("#>-"),
    );

    let part = multipart::Part::bytes(data)
        .file_name(file_name.clone())
        .mime_str("application/octet-stream")?;
    let form = multipart::Form::new().part("file", part);

    let resp = client
        .post(format!("{server}/api/upload"))
        .multipart(form)
        .send()
        .await
        .context("Failed to connect to server")?;

    pb.finish_with_message("uploaded");

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("Upload failed: {text}");
    }

    let info: UploadResponse = resp.json().await?;
    println!("Uploaded: {} (id: {})", info.name, info.id);
    Ok(())
}

pub async fn download(client: &Client, server: &str, id: &str, output: Option<&Path>) -> Result<()> {
    // First get metadata for filename
    let meta_resp = client
        .get(format!("{server}/api/files/{id}"))
        .send()
        .await
        .context("Failed to connect to server")?;

    if !meta_resp.status().is_success() {
        let text = meta_resp.text().await.unwrap_or_default();
        bail!("File not found: {text}");
    }

    let meta: FileMetadata = meta_resp.json().await?;

    let resp = client
        .get(format!("{server}/api/download/{id}"))
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("Download failed: {text}");
    }

    let pb = ProgressBar::new(meta.size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
            .progress_chars("#>-"),
    );

    let data = resp.bytes().await?;
    pb.set_position(data.len() as u64);
    pb.finish_with_message("downloaded");

    let out_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| Path::new(&meta.name).to_path_buf());

    fs::write(&out_path, &data).await?;
    println!("Saved to: {}", out_path.display());
    Ok(())
}

pub async fn list(client: &Client, server: &str) -> Result<()> {
    let resp = client
        .get(format!("{server}/api/files"))
        .send()
        .await
        .context("Failed to connect to server")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("List failed: {text}");
    }

    let list: FileListResponse = resp.json().await?;

    if list.files.is_empty() {
        println!("No files on server.");
        return Ok(());
    }

    println!("{:<38} {:<30} {:>10}  {}", "ID", "Name", "Size", "Uploaded");
    println!("{}", "-".repeat(95));
    for f in &list.files {
        let size_str = humanize_bytes(f.size);
        println!(
            "{:<38} {:<30} {:>10}  {}",
            f.id,
            truncate(&f.name, 28),
            size_str,
            f.uploaded_at.format("%Y-%m-%d %H:%M")
        );
    }
    println!("\n{} file(s)", list.files.len());
    Ok(())
}

pub async fn delete(client: &Client, server: &str, id: &str) -> Result<()> {
    let resp = client
        .delete(format!("{server}/api/files/{id}"))
        .send()
        .await
        .context("Failed to connect to server")?;

    if resp.status().is_success() {
        println!("Deleted: {id}");
    } else {
        let text = resp.text().await.unwrap_or_default();
        bail!("Delete failed: {text}");
    }
    Ok(())
}

pub async fn share(client: &Client, server: &str, id: &str) -> Result<()> {
    let resp = client
        .post(format!("{server}/api/share/{id}"))
        .send()
        .await
        .context("Failed to connect to server")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("Share failed: {text}");
    }

    let body: serde_json::Value = resp.json().await?;
    let share_url = body["share_url"].as_str().unwrap_or("?");
    println!("Share link: {server}{share_url}");
    Ok(())
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
