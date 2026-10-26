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

    // 1. Image conversions via `image` crate
    if cat == "image" {
        match target_ext.as_str() {
            "jpg" | "jpeg" => {
                let img = image::open(src).map_err(|e| e.to_string())?;
                let rgb = img.to_rgb8();
                rgb.save_with_format(&out, image::ImageFormat::Jpeg)
                    .map_err(|e| e.to_string())?;
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
            _ => {}
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
