use anyhow::{Context, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use reqwest::multipart;
use rshare_common::{FileMetadata, UploadResponse};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs;

pub async fn upload(
    client: &Client,
    server: &str,
    file_path: &Path,
    token: Option<&str>,
    delete_tokens: &mut std::collections::HashMap<String, String>,
) -> Result<()> {
    let file_name = file_path
        .file_name()
        .context("Invalid file path")?
        .to_string_lossy()
        .to_string();

    let data = fs::read(file_path).await.context("Failed to read file")?;

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

    let mut req = client.post(format!("{server}/api/upload")).multipart(form);
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }

    let resp = req.send().await.context("Failed to connect to server")?;

    pb.finish_with_message("uploaded");

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("Upload failed: {text}");
    }

    let info: UploadResponse = resp.json().await?;
    println!("Uploaded: {} (id: {})", info.name, info.id);
    println!("SHA-256:  {}", info.sha256);
    println!("Delete token saved (use: rshare delete {})", info.id);
    delete_tokens.insert(info.id.to_string(), info.delete_token);
    Ok(())
}

pub async fn download(
    client: &Client,
    server: &str,
    id: &str,
    output: Option<&Path>,
) -> Result<()> {
    // First get metadata for filename and total size
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

    let out_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| Path::new(&meta.name).to_path_buf());

    // Check if partial file exists for resume
    let mut existing_len = if out_path.exists() {
        fs::metadata(&out_path).await?.len()
    } else {
        0
    };

    let mut req = client.get(format!("{server}/api/download/{id}"));
    if existing_len > 0 && existing_len < meta.size {
        println!("Resuming download from byte {existing_len}...");
        req = req.header("Range", format!("bytes={existing_len}-"));
    } else if existing_len == meta.size {
        // Verify checksum before declaring complete
        if let Some(expected_sha256) = &meta.sha256 {
            let file_data = fs::read(&out_path).await?;
            let actual = format!("{:x}", Sha256::digest(&file_data));
            if actual == *expected_sha256 {
                println!("File already fully downloaded: {}", out_path.display());
                println!("Checksum OK: {actual}");
                return Ok(());
            }
            eprintln!("WARNING: Existing file checksum mismatch, re-downloading...");
            existing_len = 0; // Force full re-download
        } else {
            println!("File already fully downloaded: {}", out_path.display());
            return Ok(());
        }
    }

    let resp = req.send().await?;

    if !resp.status().is_success() && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        let text = resp.text().await.unwrap_or_default();
        bail!("Download failed: {text}");
    }

    let is_resume = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;

    let pb = ProgressBar::new(meta.size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
            .progress_chars("#>-"),
    );
    if is_resume {
        pb.set_position(existing_len);
    }

    let data = resp.bytes().await?;
    pb.set_position(existing_len + data.len() as u64);
    pb.finish_with_message("downloaded");

    if is_resume {
        // Append to existing file
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&out_path)
            .await?;
        file.write_all(&data).await?;
    } else {
        fs::write(&out_path, &data).await?;
    }

    println!("Saved to: {}", out_path.display());

    // Checksum verification
    if let Some(expected_sha256) = &meta.sha256 {
        let file_data = fs::read(&out_path).await?;
        let actual = format!("{:x}", Sha256::digest(&file_data));
        if actual == *expected_sha256 {
            println!("Checksum OK: {actual}");
        } else {
            eprintln!("WARNING: Checksum mismatch!");
            eprintln!("  Expected: {expected_sha256}");
            eprintln!("  Actual:   {actual}");
        }
    }

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

    let body: serde_json::Value = resp.json().await?;
    let files: Vec<FileMetadata> = serde_json::from_value(body["files"].clone())
        .context("Server response missing or invalid 'files' field")?;
    let total = body["total"].as_u64().unwrap_or(files.len() as u64);
    let page = body["page"].as_u64().unwrap_or(1);
    let per_page = body["per_page"].as_u64().unwrap_or(50);

    if files.is_empty() {
        println!("No files on server.");
        return Ok(());
    }

    println!("{:<38} {:<30} {:>10}  Uploaded", "ID", "Name", "Size");
    println!("{}", "-".repeat(95));
    for f in &files {
        let size_str = humanize_bytes(f.size);
        println!(
            "{:<38} {:<30} {:>10}  {}",
            f.id,
            truncate(&f.name, 28),
            size_str,
            f.uploaded_at.format("%Y-%m-%d %H:%M")
        );
    }
    println!(
        "\nShowing {} of {} file(s) (page {}, {} per page)",
        files.len(),
        total,
        page,
        per_page
    );
    Ok(())
}

pub async fn delete(client: &Client, server: &str, id: &str, token: Option<&str>) -> Result<()> {
    let mut req = client.delete(format!("{server}/api/files/{id}"));
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    let resp = req.send().await.context("Failed to connect to server")?;

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
        let mut end = max - 3;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}
