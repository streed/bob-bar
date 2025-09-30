use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use image::GenericImageView;

pub fn capture_screenshot() -> Result<PathBuf> {
    eprintln!("[Screenshot] Starting screenshot capture...");

    // Generate temp file path
    let temp_path = std::env::temp_dir().join(format!("bob-bar-screenshot-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ));

    eprintln!("[Screenshot] Output path: {}", temp_path.display());

    // Try Wayland first (grim)
    let wayland_result = Command::new("grim")
        .arg(&temp_path)
        .output();

    if let Ok(output) = wayland_result {
        if output.status.success() && temp_path.exists() {
            eprintln!("[Screenshot] Screenshot captured with grim (Wayland)");
            return Ok(temp_path);
        }
        eprintln!("[Screenshot] grim failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Try X11 fallback (scrot)
    eprintln!("[Screenshot] Trying X11 fallback (scrot)...");
    let x11_result = Command::new("scrot")
        .arg(&temp_path)
        .output();

    if let Ok(output) = x11_result {
        if output.status.success() && temp_path.exists() {
            eprintln!("[Screenshot] Screenshot captured with scrot (X11)");
            return Ok(temp_path);
        }
        eprintln!("[Screenshot] scrot failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Try gnome-screenshot as last resort
    eprintln!("[Screenshot] Trying gnome-screenshot fallback...");
    let gnome_result = Command::new("gnome-screenshot")
        .arg("-f")
        .arg(&temp_path)
        .output();

    if let Ok(output) = gnome_result {
        if output.status.success() && temp_path.exists() {
            eprintln!("[Screenshot] Screenshot captured with gnome-screenshot");
            return Ok(temp_path);
        }
        eprintln!("[Screenshot] gnome-screenshot failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    Err(anyhow::anyhow!(
        "Failed to capture screenshot. Please install one of: grim (Wayland), scrot (X11), or gnome-screenshot"
    ))
}

pub fn encode_image_base64(path: &PathBuf) -> Result<String> {
    let mut img = image::open(path)
        .context("Failed to open image")?;

    // Maximum dimensions
    const MAX_WIDTH: u32 = 1120;
    const MAX_HEIGHT: u32 = 1120;

    let (width, height) = img.dimensions();
    eprintln!("[Screenshot] Original dimensions: {}x{}", width, height);

    // Check if resizing is needed
    if width > MAX_WIDTH || height > MAX_HEIGHT {
        // Calculate scaling factor to maintain aspect ratio
        let width_ratio = MAX_WIDTH as f32 / width as f32;
        let height_ratio = MAX_HEIGHT as f32 / height as f32;
        let scale = width_ratio.min(height_ratio);

        let new_width = (width as f32 * scale) as u32;
        let new_height = (height as f32 * scale) as u32;

        eprintln!("[Screenshot] Resizing to: {}x{} (scale: {:.2})", new_width, new_height, scale);

        img = img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3);
    } else {
        eprintln!("[Screenshot] No resizing needed");
    }

    let mut buffer = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buffer), image::ImageFormat::Png)
        .context("Failed to encode image")?;

    eprintln!("[Screenshot] Encoded image size: {} bytes", buffer.len());

    Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, buffer))
}