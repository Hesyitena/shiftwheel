//! Image loading/encoding for conversions and tools. The pixel work lives in the `imgcore`
//! crate (built for speed); this module adds the ffmpeg fallback for formats it can't handle.

use crate::external::{self, Program, TempDir};
use image::DynamicImage;
use std::path::Path;

pub use imgcore::{JPEG_QUALITY, flatten, pdf_from_images, preview_data_url, trim_box};

/// Load any image, upright. Formats the `image` crate can't read (HEIC, AVIF, SVG, ...) go
/// through ffmpeg.
pub fn load_image(path: &Path) -> Result<DynamicImage, String> {
    match imgcore::decode(path) {
        Ok(img) => Ok(img),
        Err(native_err) => {
            let Ok(mut ffmpeg) = external::command(Program::Ffmpeg) else {
                return Err(format!("Can't read this image ({native_err}). Install FFmpeg for more formats"));
            };
            let tmp = TempDir::new("decode")?;
            let png = tmp.path().join("frame.png");
            ffmpeg
                .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
                .arg(path)
                .args(["-frames:v", "1"])
                .arg(&png);
            external::run(ffmpeg, "FFmpeg").map_err(|e| format!("Can't read this image: {e}"))?;
            imgcore::decode(&png)
        }
    }
}

/// Encode to memory in the format of `ext` (lowercase, no dot). Formats `imgcore` can't write
/// (AVIF, ...) are encoded by ffmpeg from a lossless intermediate.
pub fn encode_image(img: &DynamicImage, ext: &str, quality: u8, small_png: bool) -> Result<Vec<u8>, String> {
    if let Some(bytes) = imgcore::encode(img, ext, quality, small_png)? {
        return Ok(bytes);
    }
    let mut ffmpeg = external::command(Program::Ffmpeg).map_err(|e| format!("{e}; it is needed to write .{ext} files"))?;
    let tmp = TempDir::new("encode")?;
    let png = tmp.path().join("frame.png");
    let out = tmp.path().join(format!("out.{ext}"));
    let lossless = imgcore::encode(&imgcore::to_8bit(img), "png", 0, false)?.ok_or("PNG encoder missing")?;
    std::fs::write(&png, lossless).map_err(|e| e.to_string())?;
    ffmpeg
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
        .arg(&png)
        .args(["-frames:v", "1"]);
    if ext == "avif" {
        ffmpeg.args(["-still-picture", "1", "-pix_fmt", "yuv420p"]);
    }
    ffmpeg.arg(&out);
    external::run(ffmpeg, "FFmpeg")?;
    std::fs::read(&out).map_err(|e| e.to_string())
}
