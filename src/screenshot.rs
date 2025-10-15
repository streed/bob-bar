use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use image::GenericImageView;
use crate::config::Config;

pub fn capture_screenshot() -> Result<PathBuf> {
    capture_screenshot_with_options(false)
}

pub fn capture_screenshot_region() -> Result<PathBuf> {
    capture_screenshot_with_options(true)
}

fn capture_screenshot_with_options(region_mode: bool) -> Result<PathBuf> {
    eprintln!("[Screenshot] Starting screenshot capture{}...", if region_mode { " (region mode)" } else { "" });

    // Ensure images directory exists
    let images_dir = Config::get_config_dir().join("images");
    std::fs::create_dir_all(&images_dir)
        .context("Failed to create images directory")?;

    // Generate file path in config directory
    let screenshot_path = images_dir.join(format!("screenshot-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ));

    eprintln!("[Screenshot] Output path: {}", screenshot_path.display());

    // Try Wayland first (grim)
    let mut grim_cmd = Command::new("grim");
    if region_mode {
        grim_cmd.arg("-g");
        grim_cmd.arg("-");  // Use slurp to select region
        // On Wayland with grim, we need slurp for region selection
        // Run: grim -g "$(slurp)" output.png
        let slurp_result = Command::new("slurp").output();
        if let Ok(slurp_output) = slurp_result {
            if slurp_output.status.success() {
                let geometry = String::from_utf8_lossy(&slurp_output.stdout).trim().to_string();
                eprintln!("[Screenshot] Selected region: {}", geometry);
                let wayland_result = Command::new("grim")
                    .arg("-g")
                    .arg(geometry)
                    .arg(&screenshot_path)
                    .output();

                if let Ok(output) = wayland_result {
                    if output.status.success() && screenshot_path.exists() {
                        eprintln!("[Screenshot] Screenshot captured with grim + slurp (Wayland)");
                        return Ok(screenshot_path);
                    }
                    eprintln!("[Screenshot] grim failed: {}", String::from_utf8_lossy(&output.stderr));
                }
            } else {
                eprintln!("[Screenshot] slurp selection cancelled or failed");
                return Err(anyhow::anyhow!("Region selection cancelled"));
            }
        }
    } else {
        let wayland_result = Command::new("grim")
            .arg(&screenshot_path)
            .output();

        if let Ok(output) = wayland_result {
            if output.status.success() && screenshot_path.exists() {
                eprintln!("[Screenshot] Screenshot captured with grim (Wayland)");
                return Ok(screenshot_path);
            }
            eprintln!("[Screenshot] grim failed: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    // Try X11 fallback (scrot)
    eprintln!("[Screenshot] Trying X11 fallback (scrot)...");
    let mut scrot_cmd = Command::new("scrot");
    if region_mode {
        scrot_cmd.arg("-s");  // Select region interactively
    }
    let x11_result = scrot_cmd
        .arg(&screenshot_path)
        .output();

    if let Ok(output) = x11_result {
        if output.status.success() && screenshot_path.exists() {
            eprintln!("[Screenshot] Screenshot captured with scrot (X11)");
            return Ok(screenshot_path);
        }
        eprintln!("[Screenshot] scrot failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Try gnome-screenshot as last resort
    eprintln!("[Screenshot] Trying gnome-screenshot fallback...");
    let mut gnome_cmd = Command::new("gnome-screenshot");
    gnome_cmd.arg("-f").arg(&screenshot_path);
    if region_mode {
        gnome_cmd.arg("-a");  // Area selection
    }
    let gnome_result = gnome_cmd.output();

    if let Ok(output) = gnome_result {
        if output.status.success() && screenshot_path.exists() {
            eprintln!("[Screenshot] Screenshot captured with gnome-screenshot");
            return Ok(screenshot_path);
        }
        eprintln!("[Screenshot] gnome-screenshot failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    Err(anyhow::anyhow!(
        "Failed to capture screenshot. Please install one of: grim + slurp (Wayland), scrot (X11), or gnome-screenshot"
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