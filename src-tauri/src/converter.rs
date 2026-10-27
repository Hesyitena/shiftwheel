//! File conversions and tools. Every function writes new files next to the source and
//! never overwrites the source or an existing file.

use crate::actions::{unique_path, write_unique};
use crate::external::{self, Program, TempDir};
use crate::imaging::{self, encode_image, load_image};
use crate::pdfops;
use image::DynamicImage;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

/// Result of a job on one input.
#[derive(Debug)]
pub enum Outcome {
    /// New files that were written.
    Files(Vec<PathBuf>),
    /// Nothing written on purpose (e.g. compressing would not make the file smaller).
    Skipped(String),
}

/// Same format under different extensions (jpeg = jpg, ...).
pub fn normalize_ext(ext: &str) -> String {
    let e = ext.to_lowercase();
    match e.as_str() {
        "jpeg" => "jpg".into(),
        "tif" => "tiff".into(),
        "htm" => "html".into(),
        "aif" => "aiff".into(),
        _ => e,
    }
}

pub fn ext_of(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).map(normalize_ext).unwrap_or_default()
}

pub fn category_of(file: &str) -> &'static str {
    let path = Path::new(file);
    if path.is_dir() {
        return "folder";
    }
    match ext_of(path).as_str() {
        "png" | "jpg" | "webp" | "gif" | "tiff" | "avif" | "bmp" | "heic" | "heif" | "svg" => "image",
        "pdf" => "pdf",
        "doc" | "docx" | "odt" | "rtf" | "txt" | "html" | "md" => "document",
        "xls" | "xlsx" | "ods" | "csv" => "sheet",
        "ppt" | "pptx" | "odp" => "slides",
        "mp3" | "m4a" | "wav" | "flac" | "ogg" | "opus" | "aiff" | "wma" | "aac" => "audio",
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "m4v" => "video",
        _ if looks_like_text(path) => "text",
        _ => "unknown",
    }
}

/// Plain-text files with other extensions (.json, .log, .xml, source code, ...).
fn looks_like_text(path: &Path) -> bool {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else { return false };
    let mut buf = Vec::with_capacity(8192);
    if file.take(8192).read_to_end(&mut buf).is_err() || buf.is_empty() {
        return false;
    }
    if buf.contains(&0) {
        return false;
    }
    let control = buf.iter().filter(|&&b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r' | 0x0c)).count();
    control * 100 <= buf.len()
}

fn parts(src: &Path) -> (PathBuf, String) {
    let dir = src.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
    (dir, stem)
}

/// Move a finished temp file to a free name in `dir` (the name is claimed first, then replaced).
fn move_into(tmp: &Path, dir: &Path, stem: &str, ext: &str) -> Result<PathBuf, String> {
    let out = write_unique(dir, stem, ext, &[])?;
    if std::fs::rename(tmp, &out).is_err() {
        // Different drive than the temp folder: copy instead.
        if let Err(e) = std::fs::copy(tmp, &out) {
            let _ = std::fs::remove_file(&out);
            return Err(format!("Could not save {}: {e}", out.display()));
        }
        let _ = std::fs::remove_file(tmp);
    }
    crate::actions::notify_shell_change(&out);
    Ok(out)
}

/// Image extensions the `image` crate can write itself.
fn native_image_ext(ext: &str) -> bool {
    matches!(ext, "jpg" | "png" | "webp" | "gif" | "bmp" | "tiff")
}

// ---------------------------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------------------------

pub fn convert_file(src: &Path, target: &str) -> Result<Vec<PathBuf>, String> {
    if !src.exists() {
        return Err("The file no longer exists".into());
    }
    if src.is_dir() {
        return Err("Folders can't be converted".into());
    }
    let target = normalize_ext(target);
    if ext_of(src) == target {
        return Err(format!("This file is already {}", target.to_uppercase()));
    }
    let (dir, stem) = parts(src);

    match category_of(&src.to_string_lossy()) {
        "image" => convert_image(src, &dir, &stem, &target),
        "audio" | "video" => convert_av(src, &dir, &stem, &target).map(|p| vec![p]),
        "pdf" => convert_pdf(src, &dir, &stem, &target),
        "text" => convert_office(src, &dir, &stem, &target, Some("Text (encoded):UTF8")).map(|p| vec![p]),
        "unknown" => Err(format!("Shiftwheel can't convert .{} files", ext_of(src))),
        _ => convert_office(src, &dir, &stem, &target, None).map(|p| vec![p]),
    }
}

fn convert_image(src: &Path, dir: &Path, stem: &str, target: &str) -> Result<Vec<PathBuf>, String> {
    let img = load_image(src)?;
    let bytes = match target {
        "pdf" => imaging::pdf_from_images(std::slice::from_ref(&img))?,
        "txt" => return ocr_to_txt(&img, dir, stem).map(|p| vec![p]),
        _ => encode_image(&img, target, imaging::JPEG_QUALITY, false)?,
    };
    Ok(vec![write_unique(dir, stem, target, &bytes)?])
}

fn ffmpeg_base() -> Result<Command, String> {
    let mut cmd = external::command(Program::Ffmpeg)?;
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
    Ok(cmd)
}

/// Encoder settings for an output container.
fn av_codec_args(target: &str) -> Vec<&'static str> {
    match target {
        "mp3" => vec!["-vn", "-c:a", "libmp3lame", "-q:a", "2"],
        "m4a" | "aac" => vec!["-vn", "-c:a", "aac", "-b:a", "192k"],
        "wav" => vec!["-vn", "-c:a", "pcm_s16le"],
        "flac" => vec!["-vn", "-c:a", "flac"],
        "ogg" => vec!["-vn", "-c:a", "libvorbis", "-q:a", "5"],
        "opus" => vec!["-vn", "-c:a", "libopus", "-b:a", "128k"],
        "aiff" => vec!["-vn", "-c:a", "pcm_s16be"],
        "wma" => vec!["-vn", "-c:a", "wmav2", "-b:a", "192k"],
        "mp4" | "mov" | "m4v" => vec![
            "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k",
            "-movflags", "+faststart",
        ],
        "mkv" => vec!["-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k"],
        "webm" => vec!["-c:v", "libvpx-vp9", "-crf", "32", "-b:v", "0", "-c:a", "libopus", "-b:a", "128k"],
        "gif" => vec![
            "-an", "-vf",
            "fps=12,scale='min(480,iw)':-2:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse",
            "-loop", "0",
        ],
        _ => vec![],
    }
}

/// Run ffmpeg into a fresh output file; removes the partial file on failure.
fn run_ffmpeg(cmd: Command, out: &Path) -> Result<(), String> {
    let res = external::run(cmd, "FFmpeg").map(|_| ());
    if res.is_err() {
        let _ = std::fs::remove_file(out);
    }
    res
}

fn convert_av(src: &Path, dir: &Path, stem: &str, target: &str) -> Result<PathBuf, String> {
    let out = unique_path(dir, stem, target);
    let mut cmd = ffmpeg_base()?;
    cmd.arg("-i").arg(src).args(av_codec_args(target)).arg(&out);
    run_ffmpeg(cmd, &out)?;
    Ok(out)
}

fn convert_pdf(src: &Path, dir: &Path, stem: &str, target: &str) -> Result<Vec<PathBuf>, String> {
    match target {
        "jpg" | "png" => pdf_to_images(src, dir, stem, target, None),
        "txt" => pdf_to_text(src, dir, stem).map(|p| vec![p]),
        _ => convert_office(src, dir, stem, target, Some("writer_pdf_import")).map(|p| vec![p]),
    }
}

/// PDF text in reading order: Poppler's pdftotext when available, otherwise LibreOffice's PDF
/// import (which puts every text run in a positioned frame) sorted by page and position.
fn pdf_to_text(src: &Path, dir: &Path, stem: &str) -> Result<PathBuf, String> {
    let text = if let Ok(mut cmd) = external::command(Program::Pdftotext) {
        cmd.args(["-enc", "UTF-8"]).arg(src).arg("-");
        let out = external::run(cmd, "pdftotext")?;
        String::from_utf8_lossy(&out.stdout).replace('\u{c}', "\n")
    } else {
        pdf_text_via_office(src)?
    };
    if text.trim().is_empty() {
        return Err("This PDF has no text layer (it may be a scanned image)".into());
    }
    let out = unique_path(dir, stem, "txt");
    let text = text.replace("\r\n", "\n");
    std::fs::write(&out, text.trim_end().to_string() + "\n").map_err(|e| e.to_string())?;
    Ok(out)
}

/// Render PDF pages. One page → `name.png`, several → `name (page N).png`.
fn pdf_to_images(src: &Path, dir: &Path, stem: &str, target: &str, suffix: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let Ok(mut cmd) = external::command(Program::Pdftoppm) else {
        // Without Poppler, LibreOffice can still render the first page.
        let out = convert_office(src, dir, stem, target, None)?;
        return Ok(vec![out]);
    };
    let tmp = TempDir::new("pdftoppm")?;
    cmd.args(["-r", "150"]);
    if target == "jpg" {
        cmd.args(["-jpeg", "-jpegopt", "quality=90"]);
    } else {
        cmd.arg("-png");
    }
    cmd.arg(src).arg(tmp.path().join("page"));
    external::run(cmd, "pdftoppm")?;

    let mut pages: Vec<PathBuf> = std::fs::read_dir(tmp.path())
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    // page-01.png, page-02.png ... (zero padded, so a plain sort is page order)
    pages.sort();
    if pages.is_empty() {
        return Err("The PDF has no pages".into());
    }
    let single = pages.len() == 1;
    let mut outs = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let name = match (single, suffix) {
            (true, None) => stem.to_string(),
            (true, Some(s)) => format!("{stem} ({s})"),
            (false, _) => format!("{stem} (page {})", i + 1),
        };
        outs.push(move_into(page, dir, &name, target)?);
    }
    Ok(outs)
}

/// PDF text through LibreOffice's PDF import (used when Poppler isn't installed).
pub(crate) fn pdf_text_via_office(src: &Path) -> Result<String, String> {
    let tmp = TempDir::new("pdftext")?;
    let fodt = convert_office_to(src, tmp.path(), "fodt", Some("writer_pdf_import"))?;
    let xml = std::fs::read_to_string(&fodt).map_err(|e| e.to_string())?;
    Ok(crate::fodt::frames_text(&xml))
}

/// LibreOffice export filter for a target format, when the plain extension is ambiguous.
fn office_filter(target: &str, src_cat: &str) -> String {
    match (target, src_cat) {
        ("txt", _) => "txt:Text (encoded):UTF8".into(),
        ("csv", _) => "csv:Text - txt - csv (StarCalc):44,34,76,1".into(),
        ("html", "sheet") => "html:HTML (StarCalc)".into(),
        ("html", _) => "html:HTML (StarWriter)".into(),
        _ => target.to_string(),
    }
}

/// LibreOffice allows only one headless instance per user profile; run conversions one at a time.
static SOFFICE_LOCK: Mutex<()> = Mutex::new(());

/// Convert with LibreOffice in a private profile and output folder, then move the result next to
/// the source under a free name. LibreOffice itself would overwrite `name.pdf` if it existed.
fn convert_office(src: &Path, dir: &Path, stem: &str, target: &str, infilter: Option<&str>) -> Result<PathBuf, String> {
    let out_dir = TempDir::new("soffice")?;
    let file = convert_office_to(src, out_dir.path(), target, infilter)?;
    move_into(&file, dir, stem, target)
}

/// Run LibreOffice and return the file it wrote into `out_dir`.
fn convert_office_to(src: &Path, out_dir: &Path, target: &str, infilter: Option<&str>) -> Result<PathBuf, String> {
    let soffice = external::command(Program::Soffice)
        .map_err(|e| format!("{e}; it is needed for {} files", target.to_uppercase()))?;
    let _guard = SOFFICE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // A dedicated profile so conversions work while the user has LibreOffice open.
    let profile = std::env::temp_dir().join("shiftwheel-libreoffice-profile");
    let src_cat = category_of(&src.to_string_lossy());

    let mut cmd = soffice;
    cmd.arg(format!("-env:UserInstallation={}", external::file_url(&profile)))
        .args(["--headless", "--norestore", "--nolockcheck"]);
    if let Some(f) = infilter {
        cmd.arg(format!("--infilter={f}"));
    }
    cmd.arg("--convert-to")
        .arg(office_filter(target, src_cat))
        .arg("--outdir")
        .arg(out_dir)
        .arg(src);
    external::run_with_timeout(cmd, "LibreOffice", Some(Duration::from_secs(180)))?;

    let produced = std::fs::read_dir(out_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.is_file());
    match produced {
        Some(file) => Ok(file),
        None => Err(format!(
            "LibreOffice can't convert .{} to {}",
            ext_of(src),
            target.to_uppercase()
        )),
    }
}

// ---------------------------------------------------------------------------------------------
// OCR (Windows.Media.Ocr through PowerShell)
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
const OCR_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics, ContentType = WindowsRuntime]
$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
  $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
function Await($op, [Type]$type) { $t = $asTask.MakeGenericMethod($type).Invoke($null, @($op)); $t.Wait(-1) | Out-Null; $t.Result }
$file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync($env:SHIFTWHEEL_OCR_IMAGE)) ([Windows.Storage.StorageFile])
$stream = Await ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])
$decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
$bitmap = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
$engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
if ($engine -eq $null) { throw 'No OCR language is installed' }
$result = Await ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
$stream.Dispose()
foreach ($line in $result.Lines) { [Console]::Out.WriteLine($line.Text) }
"#;

#[cfg_attr(not(windows), allow(dead_code))]
/// Prepare an image for OCR: small images are upscaled 2x, huge ones shrunk to the engine limit.
pub fn ocr_input(img: &DynamicImage) -> DynamicImage {
    let max = img.width().max(img.height());
    let rgb = DynamicImage::ImageRgb8(imaging::flatten(img));
    if max < 1500 {
        rgb.resize(img.width() * 2, img.height() * 2, image::imageops::FilterType::CatmullRom)
    } else if max > 9000 {
        rgb.resize(9000, 9000, image::imageops::FilterType::Triangle)
    } else {
        rgb
    }
}

#[cfg(windows)]
fn ocr_text(img: &DynamicImage) -> Result<String, String> {
    let tmp = TempDir::new("ocr")?;
    let png = tmp.path().join("ocr.png");
    ocr_input(img).save_with_format(&png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    let mut cmd = external::hidden(Command::new("powershell.exe"));
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", OCR_SCRIPT])
        .env("SHIFTWHEEL_OCR_IMAGE", &png);
    let out = external::run_with_timeout(cmd, "Windows OCR", Some(Duration::from_secs(90)))?;
    Ok(String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"))
}

#[cfg(not(windows))]
fn ocr_text(_img: &DynamicImage) -> Result<String, String> {
    Err("Text recognition needs Windows 10 or 11".into())
}

fn ocr_to_txt(img: &DynamicImage, dir: &Path, stem: &str) -> Result<PathBuf, String> {
    let text = ocr_text(img)?;
    if text.trim().is_empty() {
        return Err("No text found in this image".into());
    }
    write_unique(dir, stem, "txt", (text.trim_end().to_string() + "\n").as_bytes())
}

// ---------------------------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------------------------

/// Output format for an image tool: the source format when we can write it, PNG otherwise.
fn image_tool_ext(src: &Path) -> String {
    let ext = ext_of(src);
    if native_image_ext(&ext) { ext } else { "png".into() }
}

fn write_tool_output(src: &Path, suffix: &str, ext: &str, bytes: &[u8]) -> Result<Outcome, String> {
    let (dir, stem) = parts(src);
    Ok(Outcome::Files(vec![write_unique(&dir, &format!("{stem} ({suffix})"), ext, bytes)?]))
}

fn save_tool_image(img: &DynamicImage, src: &Path, suffix: &str) -> Result<Outcome, String> {
    let ext = image_tool_ext(src);
    write_tool_output(src, suffix, &ext, &encode_image(img, &ext, imaging::JPEG_QUALITY, false)?)
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

const ALREADY_SMALL: &str = "Already as small as it gets";

fn compress_image(src: &Path) -> Result<Outcome, String> {
    let mut img = load_image(src)?;
    if img.width().max(img.height()) > 2560 {
        img = img.resize(2560, 2560, image::imageops::FilterType::Lanczos3);
    }
    let ext = image_tool_ext(src);
    let bytes = encode_image(&img, &ext, 80, true)?;
    if bytes.len() as u64 >= file_size(src) {
        return Ok(Outcome::Skipped(ALREADY_SMALL.into()));
    }
    write_tool_output(src, "compressed", &ext, &bytes)
}

/// Media duration in seconds (ffprobe).
fn duration_secs(src: &Path) -> Result<f64, String> {
    let mut cmd = external::command(Program::Ffprobe)?;
    cmd.args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(src);
    let out = external::run(cmd, "ffprobe")?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .map_err(|_| "Could not read the length of this file".to_string())
}

fn has_audio(src: &Path) -> bool {
    let Ok(mut cmd) = external::command(Program::Ffprobe) else {
        return true;
    };
    cmd.args(["-v", "error", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0"])
        .arg(src);
    external::run(cmd, "ffprobe")
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(true)
}

/// ffmpeg tool with a `name (suffix).ext` output in the source format.
fn av_tool(src: &Path, suffix: &str, out_ext: Option<&str>, args: &[&str]) -> Result<Outcome, String> {
    let (dir, stem) = parts(src);
    let ext = out_ext.map(str::to_string).unwrap_or_else(|| ext_of(src));
    let out = unique_path(&dir, &format!("{stem} ({suffix})"), &ext);
    let mut cmd = ffmpeg_base()?;
    cmd.arg("-i").arg(src).args(args);
    cmd.arg(&out);
    run_ffmpeg(cmd, &out)?;
    Ok(Outcome::Files(vec![out]))
}

/// Re-encode settings for the source container (used by filters that can't stream-copy).
fn reencode_args(src: &Path) -> Vec<&'static str> {
    let ext = ext_of(src);
    let args = av_codec_args(&ext);
    if category_of(&src.to_string_lossy()) == "audio" {
        // keep audio-only output; codec args for audio already start with -vn
        args
    } else {
        args.into_iter().filter(|a| *a != "-vn").collect()
    }
}

/// Run one tool on one file.
pub fn run_tool(tool: &str, src: &Path) -> Result<Outcome, String> {
    if !src.exists() {
        return Err("The file no longer exists".into());
    }
    if src.is_dir() {
        return Err("This tool works on files, not folders".into());
    }
    let (dir, stem) = parts(src);
    match tool {
        "image.compress" => compress_image(src),
        "image.half" => {
            let img = load_image(src)?;
            let r = img.resize_exact((img.width() / 2).max(1), (img.height() / 2).max(1), image::imageops::FilterType::Lanczos3);
            save_tool_image(&r, src, "half")
        }
        "image.fit" => {
            let img = load_image(src)?;
            if img.width() <= 1920 && img.height() <= 1080 {
                return Ok(Outcome::Skipped("Already fits in 1920×1080".into()));
            }
            save_tool_image(&img.resize(1920, 1080, image::imageops::FilterType::Lanczos3), src, "fit")
        }
        "image.square" => {
            let img = load_image(src)?;
            let side = img.width().min(img.height());
            let sq = img.crop_imm((img.width() - side) / 2, (img.height() - side) / 2, side, side);
            save_tool_image(&sq, src, "square")
        }
        "image.rotate" => save_tool_image(&load_image(src)?.rotate90(), src, "rotated"),
        "image.flip" => save_tool_image(&load_image(src)?.fliph(), src, "flipped"),
        "image.gray" => save_tool_image(&load_image(src)?.grayscale(), src, "gray"),
        "image.trim" => {
            let img = load_image(src)?;
            match imaging::trim_box(&img, 12) {
                Some((x, y, w, h)) => save_tool_image(&img.crop_imm(x, y, w, h), src, "trimmed"),
                None => Ok(Outcome::Skipped("No plain border to remove".into())),
            }
        }
        "pdf.compress" => {
            let out = unique_path(&dir, &format!("{stem} (compressed)"), "pdf");
            if pdfops::compress(src, &out)? {
                Ok(Outcome::Files(vec![out]))
            } else {
                Ok(Outcome::Skipped(ALREADY_SMALL.into()))
            }
        }
        "pdf.split" => pdfops::split(src, &dir, &stem).map(Outcome::Files),
        "pdf.images" => pdf_to_images(src, &dir, &stem, "png", Some("page 1")).map(Outcome::Files),
        "audio.trim" => {
            let f = "silenceremove=start_periods=1:start_threshold=-50dB:start_silence=0.1,areverse,\
                     silenceremove=start_periods=1:start_threshold=-50dB:start_silence=0.1,areverse";
            let mut args = vec!["-af", f];
            args.extend(reencode_args(src));
            av_tool(src, "trimmed", None, &args)
        }
        "audio.fade" => {
            let dur = duration_secs(src)?;
            let d = (dur / 4.0).min(1.5);
            let f = format!("afade=t=in:d={d:.3},afade=t=out:st={:.3}:d={d:.3}", (dur - d).max(0.0));
            let mut args = vec!["-af", f.as_str()];
            args.extend(reencode_args(src));
            av_tool(src, "faded", None, &args)
        }
        "audio.fast" => {
            let mut args = vec!["-af", "atempo=2.0"];
            args.extend(reencode_args(src));
            av_tool(src, "2x", None, &args)
        }
        "audio.mono" => {
            let mut args = vec!["-ac", "1"];
            args.extend(reencode_args(src));
            av_tool(src, "mono", None, &args)
        }
        "video.mute" => av_tool(src, "muted", None, &["-map", "0", "-map", "-0:a", "-c", "copy"]),
        "video.audio" => av_tool(src, "audio", Some("mp3"), &av_codec_args("mp3")),
        "video.fast" => {
            let mut args = vec!["-filter:v", "setpts=0.5*PTS"];
            if has_audio(src) {
                args.extend(["-filter:a", "atempo=2.0"]);
            }
            args.extend(reencode_args(src));
            av_tool(src, "2x", None, &args)
        }
        "video.half" => {
            let mut args = vec![
                "-vf",
                "scale=w='min(1280,iw)':h='min(720,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2",
            ];
            args.extend(reencode_args(src));
            av_tool(src, "720p", None, &args)
        }
        "video.reverse" => {
            let mut args = vec!["-vf", "reverse"];
            if has_audio(src) {
                args.extend(["-af", "areverse"]);
            }
            args.extend(reencode_args(src));
            av_tool(src, "reversed", None, &args)
        }
        _ => Err(format!("Unknown tool: {tool}")),
    }
}

/// A crop selection, as fractions (0..1) of the image.
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct CropRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn crop_image(src: &Path, rect: CropRect) -> Result<Outcome, String> {
    let img = load_image(src)?;
    let (iw, ih) = (img.width(), img.height());
    let (w, h) = (iw as f64, ih as f64);
    let x = ((rect.x.clamp(0.0, 1.0) * w).round() as u32).min(iw - 1);
    let y = ((rect.y.clamp(0.0, 1.0) * h).round() as u32).min(ih - 1);
    let cw = ((rect.w.clamp(0.0, 1.0) * w).round() as u32).clamp(1, iw - x);
    let ch = ((rect.h.clamp(0.0, 1.0) * h).round() as u32).clamp(1, ih - y);
    save_tool_image(&img.crop_imm(x, y, cw, ch), src, "cropped")
}

/// Combine images into one PDF named after the first one.
pub fn images_to_combined_pdf(paths: &[PathBuf]) -> Result<Outcome, String> {
    let first = paths.first().ok_or("No files selected")?;
    let images = paths.iter().map(|p| load_image(p)).collect::<Result<Vec<_>, _>>()?;
    let (dir, stem) = parts(first);
    let pdf = imaging::pdf_from_images(&images)?;
    Ok(Outcome::Files(vec![write_unique(&dir, &format!("{stem} (combined)"), "pdf", &pdf)?]))
}

pub fn merge_pdfs(paths: &[PathBuf]) -> Result<Outcome, String> {
    let first = paths.first().ok_or("No files selected")?;
    let (dir, stem) = parts(first);
    let out = unique_path(&dir, &format!("{stem} (merged)"), "pdf");
    pdfops::merge(paths, &out)?;
    Ok(Outcome::Files(vec![out]))
}
