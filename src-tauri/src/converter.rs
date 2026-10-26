use std::path::{Path, PathBuf};
use std::process::Command;
use crate::actions::unique_path;

pub fn category_of(file: &str) -> &'static str {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "tiff" | "tif" | "avif" | "bmp" | "heic" | "svg" => "image",
        "pdf" => "pdf",
        "doc" | "docx" | "odt" | "rtf" | "txt" | "html" | "htm" | "md" => "document",
        "xls" | "xlsx" | "ods" | "csv" => "sheet",
        "ppt" | "pptx" | "odp" => "slides",
        "mp3" | "m4a" | "wav" | "flac" | "ogg" | "opus" | "aiff" | "aif" | "wma" | "aac" => "audio",
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "m4v" => "video",
        _ => "unknown",
    }
}

pub fn image_to_pdf(src_path: &Path, out_path: &Path) -> Result<(), String> {
    let img = image::open(src_path).map_err(|e| e.to_string())?;
    let (width, height) = (img.width(), img.height());

    let mut jpeg_buf = std::io::Cursor::new(Vec::new());
    let rgb = if let Some(rgba) = img.as_rgba8() {
        let mut flat = image::RgbImage::new(width, height);
        for (x, y, p) in rgba.enumerate_pixels() {
            let a = p[3] as f32 / 255.0;
            let r = (p[0] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
            let g = (p[1] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
            let b = (p[2] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
            flat.put_pixel(x, y, image::Rgb([r, g, b]));
        }
        flat
    } else {
        img.to_rgb8()
    };

    rgb.write_to(&mut jpeg_buf, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    let jpeg_bytes = jpeg_buf.into_inner();
    let jpeg_len = jpeg_bytes.len();

    let w_pt = width as f64;
    let h_pt = height as f64;

    let mut pdf = Vec::new();
    use std::io::Write;
    let _ = writeln!(pdf, "%PDF-1.4");

    let off1 = pdf.len();
    let _ = writeln!(pdf, "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj");

    let off2 = pdf.len();
    let _ = writeln!(pdf, "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj");

    let off3 = pdf.len();
    let _ = writeln!(
        pdf,
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>\nendobj",
        w_pt, h_pt
    );

    let off4 = pdf.len();
    let _ = writeln!(
        pdf,
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream",
        width, height, jpeg_len
    );
    pdf.extend_from_slice(&jpeg_bytes);
    let _ = writeln!(pdf, "\nendstream\nendobj");

    let off5 = pdf.len();
    let stream_content = format!("q {:.2} 0 0 {:.2} 0 0 cm /Im0 Do Q", w_pt, h_pt);
    let _ = writeln!(
        pdf,
        "5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj",
        stream_content.len(),
        stream_content
    );

    let xref_off = pdf.len();
    let _ = writeln!(
        pdf,
        "xref\n0 6\n0000000000 65535 f \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n ",
        off1, off2, off3, off4, off5
    );
    let _ = writeln!(
        pdf,
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
        xref_off
    );

    std::fs::write(out_path, pdf).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn convert_file(src_path: &str, target_fmt: &str) -> Result<PathBuf, String> {
    let src = Path::new(src_path);
    if !src.exists() {
        return Err("Source file does not exist".into());
    }

    let dir = src.parent().unwrap_or(Path::new("."));
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let target_ext = target_fmt.to_lowercase();
    let out = unique_path(dir, stem, &target_ext);

    let cat = category_of(src_path);

    // 1. Image conversions via `image` crate and pure Rust
    if cat == "image" {
        match target_ext.as_str() {
            "jpg" | "jpeg" => {
                let img = image::open(src).map_err(|e| e.to_string())?;
                if let Some(rgba) = img.as_rgba8() {
                    let (w, h) = rgba.dimensions();
                    let mut flat = image::RgbImage::new(w, h);
                    for (x, y, p) in rgba.enumerate_pixels() {
                        let a = p[3] as f32 / 255.0;
                        let r = (p[0] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
                        let g = (p[1] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
                        let b = (p[2] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
                        flat.put_pixel(x, y, image::Rgb([r, g, b]));
                    }
                    flat.save_with_format(&out, image::ImageFormat::Jpeg)
                        .map_err(|e| e.to_string())?;
                } else {
                    let rgb = img.to_rgb8();
                    rgb.save_with_format(&out, image::ImageFormat::Jpeg)
                        .map_err(|e| e.to_string())?;
                }
                return Ok(out);
            }
            "png" => {
                let img = image::open(src).map_err(|e| e.to_string())?;
                img.save_with_format(&out, image::ImageFormat::Png)
                    .map_err(|e| e.to_string())?;
                return Ok(out);
            }
            "webp" => {
                let img = image::open(src).map_err(|e| e.to_string())?;
                img.save_with_format(&out, image::ImageFormat::WebP)
                    .map_err(|e| e.to_string())?;
                return Ok(out);
            }
            "bmp" => {
                let img = image::open(src).map_err(|e| e.to_string())?;
                img.save_with_format(&out, image::ImageFormat::Bmp)
                    .map_err(|e| e.to_string())?;
                return Ok(out);
            }
            "tiff" | "tif" => {
                let img = image::open(src).map_err(|e| e.to_string())?;
                img.save_with_format(&out, image::ImageFormat::Tiff)
                    .map_err(|e| e.to_string())?;
                return Ok(out);
            }
            "gif" => {
                let img = image::open(src).map_err(|e| e.to_string())?;
                img.save_with_format(&out, image::ImageFormat::Gif)
                    .map_err(|e| e.to_string())?;
                return Ok(out);
            }
            "pdf" => {
                image_to_pdf(src, &out)?;
                return Ok(out);
            }
            _ => {
                // Try ffmpeg for other formats like avif
                let status = Command::new("ffmpeg")
                    .args(["-y", "-i", src_path, &out.to_string_lossy()])
                    .status();
                if let Ok(st) = status {
                    if st.success() {
                        return Ok(out);
                    }
                }
            }
        }
    }

    // 2. Audio/Video conversions via ffmpeg
    if cat == "audio" || cat == "video" {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-y", "-i", src_path]);

        match target_ext.as_str() {
            "mp3" => {
                cmd.args(["-vn", "-c:a", "libmp3lame", "-q:a", "2"]);
            }
            "m4a" => {
                cmd.args(["-vn", "-c:a", "aac", "-b:a", "192k"]);
            }
            "wav" => {
                cmd.args(["-vn", "-c:a", "pcm_s16le"]);
            }
            "flac" => {
                cmd.args(["-vn", "-c:a", "flac"]);
            }
            "ogg" => {
                cmd.args(["-vn", "-c:a", "libvorbis", "-q:a", "5"]);
            }
            "mp4" => {
                cmd.args([
                    "-c:v", "libx264", "-preset", "medium", "-crf", "20",
                    "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k",
                    "-movflags", "+faststart",
                ]);
            }
            "webm" => {
                cmd.args([
                    "-c:v", "libvpx-vp9", "-crf", "32", "-b:v", "0",
                    "-c:a", "libopus", "-b:a", "128k",
                ]);
            }
            "gif" => {
                cmd.args([
                    "-an", "-vf",
                    "fps=12,scale='min(480,iw)':-2:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse",
                    "-loop", "0",
                ]);
            }
            _ => {}
        }

        cmd.arg(&out);
        let status = cmd.status().map_err(|e| format!("ffmpeg execution failed: {e}"))?;
        if status.success() {
            return Ok(out);
        } else {
            return Err("ffmpeg returned an error".into());
        }
    }

    // 3. Document conversions via LibreOffice (soffice)
    let lo_status = Command::new("soffice")
        .args([
            "--headless",
            "--convert-to",
            &target_ext,
            src_path,
            "--outdir",
            &dir.to_string_lossy(),
        ])
        .status();

    if let Ok(st) = lo_status {
        if st.success() {
            let direct_out = dir.join(format!("{stem}.{target_ext}"));
            if direct_out.exists() {
                return Ok(direct_out);
            }
        }
    }

    Err(format!("No converter available for {} to {}", src_path, target_fmt))
}

pub fn run_tool(tool_id: &str, paths: &[String]) -> Result<PathBuf, String> {
    if paths.is_empty() {
        return Err("No files selected".into());
    }
    let src_path = &paths[0];
    let src = Path::new(src_path);
    let dir = src.parent().unwrap_or(Path::new("."));
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("png");

    match tool_id {
        "image.rotate" => {
            let img = image::open(src).map_err(|e| e.to_string())?;
            let out = unique_path(dir, &format!("{stem} (rotated)"), ext);
            let rotated = img.rotate90();
            rotated.save(&out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        "image.flip" => {
            let img = image::open(src).map_err(|e| e.to_string())?;
            let out = unique_path(dir, &format!("{stem} (flipped)"), ext);
            let flipped = img.fliph();
            flipped.save(&out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        "image.gray" => {
            let img = image::open(src).map_err(|e| e.to_string())?;
            let out = unique_path(dir, &format!("{stem} (gray)"), ext);
            let gray = img.grayscale();
            gray.save(&out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        "image.half" => {
            let img = image::open(src).map_err(|e| e.to_string())?;
            let (w, h) = (img.width() / 2, img.height() / 2);
            let out = unique_path(dir, &format!("{stem} (half)"), ext);
            let resized = img.resize(w.max(1), h.max(1), image::imageops::FilterType::Lanczos3);
            resized.save(&out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        "image.fit" => {
            let img = image::open(src).map_err(|e| e.to_string())?;
            let out = unique_path(dir, &format!("{stem} (fit)"), ext);
            let resized = img.resize(1920, 1080, image::imageops::FilterType::Lanczos3);
            resized.save(&out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        "image.square" => {
            let mut img = image::open(src).map_err(|e| e.to_string())?;
            let (w, h) = (img.width(), img.height());
            let side = w.min(h);
            let x = (w - side) / 2;
            let y = (h - side) / 2;
            let cropped = img.crop(x, y, side, side);
            let out = unique_path(dir, &format!("{stem} (square)"), ext);
            cropped.save(&out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        "video.mute" => {
            let out = unique_path(dir, &format!("{stem} (muted)"), ext);
            let status = Command::new("ffmpeg")
                .args(["-y", "-i", src_path, "-c", "copy", "-an", &out.to_string_lossy()])
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                Ok(out)
            } else {
                Err("ffmpeg mute failed".into())
            }
        }
        "video.audio" => {
            let out = unique_path(dir, &format!("{stem} (audio)"), "mp3");
            let status = Command::new("ffmpeg")
                .args(["-y", "-i", src_path, "-vn", "-c:a", "libmp3lame", "-q:a", "2", &out.to_string_lossy()])
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                Ok(out)
            } else {
                Err("ffmpeg extract audio failed".into())
            }
        }
        _ => Err(format!("Unsupported tool: {tool_id}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_conversions() {
        let temp_dir = std::env::temp_dir();
        let src_path = temp_dir.join("shiftwheel_test_input.png");

        // Create a 64x64 test image with some color & transparency
        let mut img = image::RgbaImage::new(64, 64);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x * 4) as u8, (y * 4) as u8, 128, 200]);
        }
        img.save(&src_path).expect("Failed to write test PNG");

        let src_str = src_path.to_str().unwrap();

        // Test JPG conversion (with white background flattening)
        let jpg_out = convert_file(src_str, "jpg").expect("JPG conversion failed");
        assert!(jpg_out.exists());
        let _ = std::fs::remove_file(jpg_out);

        // Test WebP conversion
        let webp_out = convert_file(src_str, "webp").expect("WebP conversion failed");
        assert!(webp_out.exists());
        let _ = std::fs::remove_file(webp_out);

        // Test BMP conversion
        let bmp_out = convert_file(src_str, "bmp").expect("BMP conversion failed");
        assert!(bmp_out.exists());
        let _ = std::fs::remove_file(bmp_out);

        // Test TIFF conversion
        let tiff_out = convert_file(src_str, "tiff").expect("TIFF conversion failed");
        assert!(tiff_out.exists());
        let _ = std::fs::remove_file(tiff_out);

        // Test GIF conversion
        let gif_out = convert_file(src_str, "gif").expect("GIF conversion failed");
        assert!(gif_out.exists());
        let _ = std::fs::remove_file(gif_out);

        // Test PDF conversion
        let pdf_out = convert_file(src_str, "pdf").expect("PDF conversion failed");
        assert!(pdf_out.exists());
        let pdf_bytes = std::fs::read(&pdf_out).expect("Read PDF bytes");
        assert!(pdf_bytes.starts_with(b"%PDF-1.4"));
        let _ = std::fs::remove_file(pdf_out);

        // Test Tools
        let rot_out = run_tool("image.rotate", &[src_str.to_string()]).expect("Rotate failed");
        assert!(rot_out.exists());
        let _ = std::fs::remove_file(rot_out);

        let half_out = run_tool("image.half", &[src_str.to_string()]).expect("Half failed");
        assert!(half_out.exists());
        let _ = std::fs::remove_file(half_out);

        let square_out = run_tool("image.square", &[src_str.to_string()]).expect("Square failed");
        assert!(square_out.exists());
        let _ = std::fs::remove_file(square_out);

        let _ = std::fs::remove_file(src_path);
    }
}
