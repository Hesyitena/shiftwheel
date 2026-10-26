//! Image loading/saving helpers shared by conversions and tools.

use crate::external::{self, Program, TempDir};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader, RgbImage};
use std::io::{BufWriter, Write};
use std::path::Path;

pub const JPEG_QUALITY: u8 = 90;

fn lower_ext(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default()
}

/// Decode with the `image` crate and apply the EXIF orientation, so phone photos come out upright.
fn load_native(path: &Path) -> Result<DynamicImage, String> {
    let reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Load any image. Formats the `image` crate can't read (HEIC, AVIF, SVG, ...) go through ffmpeg.
pub fn load_image(path: &Path) -> Result<DynamicImage, String> {
    match load_native(path) {
        Ok(img) => Ok(img),
        Err(native_err) => {
            let Ok(ffmpeg) = external::command(Program::Ffmpeg) else {
                return Err(format!("Can't read this image ({native_err}). Install FFmpeg for more formats"));
            };
            let tmp = TempDir::new("decode")?;
            let png = tmp.path().join("frame.png");
            let mut cmd = ffmpeg;
            cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
                .arg(path)
                .args(["-frames:v", "1"])
                .arg(&png);
            external::run(cmd, "FFmpeg").map_err(|e| format!("Can't read this image: {e}"))?;
            load_native(&png)
        }
    }
}

/// Composite onto white so transparent areas don't turn black in JPEG/PDF.
pub fn flatten(img: &DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = RgbImage::new(w, h);
    for (src, dst) in rgba.pixels().zip(out.pixels_mut()) {
        let a = src[3] as u32;
        let blend = |c: u8| ((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        *dst = image::Rgb([blend(src[0]), blend(src[1]), blend(src[2])]);
    }
    out
}

pub fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let rgb = flatten(img);
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Pixel layout every encoder accepts: 8-bit RGB, or RGBA when there is transparency.
fn to_8bit(img: &DynamicImage) -> DynamicImage {
    if img.color().has_alpha() {
        DynamicImage::ImageRgba8(img.to_rgba8())
    } else {
        DynamicImage::ImageRgb8(img.to_rgb8())
    }
}

/// Save in the format given by the file extension, converting the pixel layout as needed.
pub fn save_image(img: &DynamicImage, out: &Path) -> Result<(), String> {
    save_image_with(img, out, JPEG_QUALITY, false)
}

/// `quality` applies to JPEG; `small_png` uses the slowest/best PNG compression.
pub fn save_image_with(img: &DynamicImage, out: &Path, quality: u8, small_png: bool) -> Result<(), String> {
    let ext = lower_ext(out);
    let write = |bytes: &[u8]| std::fs::write(out, bytes).map_err(|e| e.to_string());
    match ext.as_str() {
        "jpg" | "jpeg" => write(&encode_jpeg(img, quality)?),
        "png" => {
            let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
            let w = BufWriter::new(file);
            let (comp, filter) = if small_png {
                (CompressionType::Best, PngFilter::Adaptive)
            } else {
                (CompressionType::Default, PngFilter::Adaptive)
            };
            let img = match img {
                DynamicImage::ImageLuma8(_)
                | DynamicImage::ImageLumaA8(_)
                | DynamicImage::ImageRgb8(_)
                | DynamicImage::ImageRgba8(_)
                | DynamicImage::ImageLuma16(_)
                | DynamicImage::ImageLumaA16(_)
                | DynamicImage::ImageRgb16(_)
                | DynamicImage::ImageRgba16(_) => img.clone(),
                _ => to_8bit(img),
            };
            PngEncoder::new_with_quality(w, comp, filter)
                .write_image(img.as_bytes(), img.width(), img.height(), img.color().into())
                .map_err(|e| e.to_string())
        }
        "webp" | "gif" | "bmp" | "tiff" | "tif" => {
            let fmt = image::ImageFormat::from_extension(&ext).ok_or("Unknown image format")?;
            to_8bit(img).save_with_format(out, fmt).map_err(|e| e.to_string())
        }
        // Everything else (AVIF, HEIC, ...) is encoded by ffmpeg from a lossless intermediate.
        _ => {
            let mut ffmpeg = external::command(Program::Ffmpeg)
                .map_err(|e| format!("{e}; it is needed to write .{ext} files"))?;
            let tmp = TempDir::new("encode")?;
            let png = tmp.path().join("frame.png");
            to_8bit(img).save_with_format(&png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
            ffmpeg
                .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
                .arg(&png)
                .args(["-frames:v", "1"]);
            if ext == "avif" {
                ffmpeg.args(["-still-picture", "1", "-pix_fmt", "yuv420p"]);
            }
            ffmpeg.arg(out);
            let res = external::run(ffmpeg, "FFmpeg");
            if res.is_err() {
                let _ = std::fs::remove_file(out);
            }
            res.map(|_| ())
        }
    }
}

/// A4 portrait in PDF points.
const A4: (f64, f64) = (595.28, 841.89);

/// Page layout for one image: an A4 page turned to match the image, with the image centered and
/// scaled to fit. Small images are never blown up past their 96-DPI size.
fn page_layout(w: u32, h: u32) -> (f64, f64, f64, f64, f64, f64) {
    let (pw, ph) = if w > h { (A4.1, A4.0) } else { A4 };
    let natural = 72.0 / 96.0; // points per pixel at 96 DPI
    let scale = (pw / w as f64).min(ph / h as f64).min(natural);
    let (iw, ih) = (w as f64 * scale, h as f64 * scale);
    (pw, ph, (pw - iw) / 2.0, (ph - ih) / 2.0, iw, ih)
}

/// Write a PDF with one page per image.
pub fn images_to_pdf(images: &[DynamicImage], out: &Path) -> Result<(), String> {
    if images.is_empty() {
        return Err("No images".into());
    }
    let mut pdf: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    let n = images.len();
    // Objects: 1 catalog, 2 pages, then per image: page, image, content.
    let page_id = |i: usize| 3 + i * 3;
    pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let obj = |pdf: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(pdf.len());
        let id = offsets.len();
        let _ = write!(pdf, "{id} 0 obj\n");
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };

    obj(&mut pdf, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", page_id(i))).collect();
    obj(&mut pdf, &mut offsets, format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).as_bytes());

    for (i, img) in images.iter().enumerate() {
        let (w, h) = (img.width(), img.height());
        let (pw, ph, x, y, iw, ih) = page_layout(w, h);
        let pid = page_id(i);
        obj(
            &mut pdf,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>",
                pid + 1,
                pid + 2
            )
            .as_bytes(),
        );
        let jpeg = encode_jpeg(img, JPEG_QUALITY)?;
        let mut body = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg.len()
        )
        .into_bytes();
        body.extend_from_slice(&jpeg);
        body.extend_from_slice(b"\nendstream");
        obj(&mut pdf, &mut offsets, &body);
        let content = format!("q {iw:.2} 0 0 {ih:.2} {x:.2} {y:.2} cm /Im0 Do Q");
        obj(&mut pdf, &mut offsets, format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).as_bytes());
    }

    let xref = pdf.len();
    let _ = write!(pdf, "xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
    for off in &offsets {
        let _ = write!(pdf, "{off:010} 00000 n \n");
    }
    let _ = write!(pdf, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1);
    std::fs::write(out, pdf).map_err(|e| e.to_string())
}

/// Bounding box of everything that differs from the border color (top-left pixel).
/// Returns None when there is no plain border to remove.
pub fn trim_box(img: &DynamicImage, tolerance: u8) -> Option<(u32, u32, u32, u32)> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w < 3 || h < 3 {
        return None;
    }
    let bg = *rgba.get_pixel(0, 0);
    let differs = |p: &image::Rgba<u8>| p.0.iter().zip(bg.0.iter()).any(|(a, b)| a.abs_diff(*b) > tolerance);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (x, y, p) in rgba.enumerate_pixels() {
        if differs(p) {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 > x1 || y0 > y1 {
        return None; // the whole image is one color
    }
    let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
    if bw == w && bh == h {
        return None;
    }
    Some((x0, y0, bw, bh))
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Downscaled JPEG data URL for previews (the crop editor).
pub fn preview_data_url(img: &DynamicImage, max_side: u32) -> Result<String, String> {
    let small = if img.width().max(img.height()) > max_side {
        img.resize(max_side, max_side, image::imageops::FilterType::Triangle)
    } else {
        img.clone()
    };
    Ok(format!("data:image/jpeg;base64,{}", base64(&encode_jpeg(&small, 85)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_handles_every_alpha_layout() {
        // 16-bit RGBA and 8-bit gray+alpha used to come out black.
        let rgba16 = DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(2, 2, image::Rgba([0u16, 0, 0, 0])));
        assert_eq!(flatten(&rgba16).get_pixel(0, 0).0, [255, 255, 255]);
        let la8 = DynamicImage::ImageLumaA8(image::ImageBuffer::from_pixel(2, 2, image::LumaA([0u8, 0])));
        assert_eq!(flatten(&la8).get_pixel(0, 0).0, [255, 255, 255]);
        let half = DynamicImage::ImageRgba8(image::ImageBuffer::from_pixel(1, 1, image::Rgba([0u8, 0, 0, 128])));
        assert_eq!(flatten(&half).get_pixel(0, 0).0, [127, 127, 127]);
    }

    #[test]
    fn pdf_page_is_a4_and_never_huge() {
        let (pw, ph, ..) = page_layout(4000, 3000);
        assert_eq!((pw, ph), (A4.1, A4.0)); // landscape A4
        let (_, _, _, _, iw, ih) = page_layout(200, 100);
        assert_eq!((iw, ih), (150.0, 75.0)); // 96 DPI, not stretched to the page
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn trim_box_finds_content() {
        let mut img = image::RgbImage::from_pixel(10, 8, image::Rgb([255, 255, 255]));
        img.put_pixel(3, 2, image::Rgb([0, 0, 0]));
        img.put_pixel(6, 5, image::Rgb([0, 0, 0]));
        assert_eq!(trim_box(&DynamicImage::ImageRgb8(img), 12), Some((3, 2, 4, 4)));
        let plain = image::RgbImage::from_pixel(10, 8, image::Rgb([9, 9, 9]));
        assert_eq!(trim_box(&DynamicImage::ImageRgb8(plain), 12), None);
    }
}
