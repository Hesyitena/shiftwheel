//! End-to-end tests of the job pipeline: a drop position on the wheel is resolved to a slice
//! exactly like the overlay does, the job runs with the real helper programs (ffmpeg,
//! LibreOffice, pdftoppm) and the files it writes are decoded and checked.
//!
//! Tests that need a helper program skip themselves (with a message) when it isn't installed.

use crate::config::{WheelItem, get_wheel_config};
use crate::converter::CropRect;
use crate::external::{self, Program, TempDir};
use crate::jobs::{CropRequest, HudMsg, Report, Ui, run_job};
use crate::{pdfops, wheel};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

// ---------------------------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct RecUi {
    hud: Mutex<Vec<HudMsg>>,
    finals: Mutex<Vec<(HudMsg, Option<PathBuf>)>>,
    sounds: Mutex<Vec<String>>,
    crop_answer: Mutex<Option<CropRect>>,
    crop_requests: Mutex<Vec<CropRequest>>,
    folder: Mutex<Option<PathBuf>>,
    clipboard: Mutex<Vec<String>>,
}

impl Ui for RecUi {
    fn hud(&self, msg: &HudMsg) {
        self.hud.lock().unwrap().push(msg.clone());
    }
    fn finish(&self, msg: &HudMsg, reveal: Option<PathBuf>) {
        self.finals.lock().unwrap().push((msg.clone(), reveal));
    }
    fn sound(&self, name: &str) {
        self.sounds.lock().unwrap().push(name.into());
    }
    fn ask_crop(&self, req: CropRequest) -> Option<CropRect> {
        self.crop_requests.lock().unwrap().push(req);
        *self.crop_answer.lock().unwrap()
    }
    fn pick_folder(&self) -> Option<PathBuf> {
        self.folder.lock().unwrap().clone()
    }
    fn copy_text(&self, text: &str) -> Result<(), String> {
        self.clipboard.lock().unwrap().push(text.into());
        Ok(())
    }
    fn copy_files(&self, paths: &[String]) -> Result<(), String> {
        self.clipboard.lock().unwrap().push(paths.join("|"));
        Ok(())
    }
}

fn have(p: Program) -> bool {
    let ok = external::find(p).is_some();
    if !ok {
        eprintln!("SKIP: {} not installed", p.display_name());
    }
    ok
}

/// Logical point in the middle of a slice, as the user would drop it.
fn point_for(items: &[WheelItem], id: &str) -> (f64, f64) {
    let i = items.iter().position(|it| it.id == id).unwrap_or_else(|| {
        panic!("{id} is not on the wheel: {:?}", items.iter().map(|i| &i.id).collect::<Vec<_>>())
    });
    let step = 360.0 / items.len() as f64;
    let a = (-90.0 + i as f64 * step).to_radians();
    let r = (wheel::R_INNER + wheel::R_OUTER) / 2.0;
    (wheel::CENTER + r * a.cos(), wheel::CENTER + r * a.sin())
}

/// Drop `paths` on the slice `id` of the `mode` wheel and run the job.
fn drop_on(mode: &str, id: &str, paths: &[&Path], ui: &RecUi) -> Report {
    let paths: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
    let items = wheel::items_for(&get_wheel_config(), mode, &paths);
    let (x, y) = point_for(&items, id);
    let resolved = wheel::resolve(&items, x, y).expect("drop resolves to a slice");
    assert_eq!(resolved, id, "the highlighted slice must be the one that runs");
    run_job(&resolved, &paths, ui)
}

fn ok(report: &Report) -> &HudMsg {
    let msg = report.final_msg.as_ref().expect("final message");
    assert_eq!(msg.state, "done", "job failed: {msg:?} / {:?}", report.errors);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    msg
}

fn one_output(report: &Report) -> PathBuf {
    let msg = ok(report);
    assert_eq!(report.outputs.len(), 1, "{:?}", report.outputs);
    let out = report.outputs[0].clone();
    assert!(out.is_file(), "{} missing", out.display());
    let len = fs::metadata(&out).unwrap().len();
    assert!(len > 0);
    // The finished card names the new file and its size
    assert_eq!(msg.output, out.file_name().unwrap().to_string_lossy());
    assert_eq!(msg.out_bytes, len);
    assert_eq!(msg.linger_ms, crate::jobs::DONE_LINGER_MS);
    out
}

fn ffmpeg(args: &[&str]) {
    let mut cmd = external::command(Program::Ffmpeg).unwrap();
    cmd.args(["-hide_banner", "-loglevel", "error", "-y"]).args(args);
    external::run(cmd, "ffmpeg fixture").unwrap();
}

fn probe(path: &Path, entries: &str, select: Option<&str>) -> String {
    let mut cmd = external::command(Program::Ffprobe).unwrap();
    cmd.args(["-v", "error"]);
    if let Some(s) = select {
        cmd.args(["-select_streams", s]);
    }
    cmd.args(["-show_entries", entries, "-of", "default=noprint_wrappers=1:nokey=1"]).arg(path);
    String::from_utf8_lossy(&external::run(cmd, "ffprobe").unwrap().stdout).trim().to_string()
}

fn duration(path: &Path) -> f64 {
    probe(path, "format=duration", None).parse().unwrap()
}

fn audio_streams(path: &Path) -> usize {
    probe(path, "stream=index", Some("a")).lines().filter(|l| !l.is_empty()).count()
}

fn test_png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let mut img = image::RgbaImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 90, 255]);
    }
    // transparent top-left corner, must become white in JPEG/PDF
    for y in 0..4.min(h) {
        for x in 0..4.min(w) {
            img.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
        }
    }
    let p = dir.join(name);
    img.save(&p).unwrap();
    p
}

/// A JPEG of `w`×`h` stored pixels with EXIF orientation 6 ("rotate 90° clockwise to view").
fn rotated_jpeg(dir: &Path, w: u32, h: u32) -> PathBuf {
    let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30])));
    let jpeg = imgcore::encode_jpeg(&img, 90).unwrap();
    // APP1 Exif: big-endian TIFF with one IFD entry, Orientation (0x0112) = 6.
    let tiff: Vec<u8> = [
        &b"MM\x00\x2a\x00\x00\x00\x08"[..],
        &[0x00, 0x01],
        &[0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x06, 0x00, 0x00],
        &[0x00, 0x00, 0x00, 0x00],
    ]
    .concat();
    let mut app1 = b"Exif\x00\x00".to_vec();
    app1.extend(tiff);
    let len = (app1.len() + 2) as u16;
    let mut out = jpeg[..2].to_vec(); // SOI
    out.extend([0xFF, 0xE1]);
    out.extend(len.to_be_bytes());
    out.extend(app1);
    out.extend(&jpeg[2..]);
    let p = dir.join("phone.jpg");
    fs::write(&p, out).unwrap();
    p
}

fn test_wav(dir: &Path) -> PathBuf {
    let p = dir.join("tone.wav");
    // 0.5 s silence, 2 s stereo tone, 0.5 s silence
    ffmpeg(&[
        "-f", "lavfi", "-i", "sine=frequency=440:duration=2:sample_rate=44100",
        "-af", "adelay=500|500,apad=pad_dur=0.5,aformat=channel_layouts=stereo",
        p.to_str().unwrap(),
    ]);
    p
}

fn test_mp4(dir: &Path) -> PathBuf {
    let p = dir.join("clip.mp4");
    ffmpeg(&[
        "-f", "lavfi", "-i", "testsrc=size=1920x1080:rate=10:duration=2",
        "-f", "lavfi", "-i", "sine=frequency=330:duration=2",
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
        p.to_str().unwrap(),
    ]);
    p
}

/// A multi-page PDF made by LibreOffice (real-world structure: inherited page attributes).
fn office_pdf(dir: &Path, name: &str, lines: usize) -> PathBuf {
    let txt = dir.join(format!("{name}.txt"));
    let body: String = (1..=lines).map(|i| format!("{name} line {i} lorem ipsum dolor sit amet\n")).collect();
    fs::write(&txt, body).unwrap();
    let r = run_job("convert:pdf", &[txt.to_string_lossy().to_string()], &RecUi::default());
    let out = one_output(&r);
    fs::remove_file(&txt).unwrap();
    out
}

// ---------------------------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------------------------

#[test]
fn e2e_image_conversions() {
    let t = TempDir::new("e2e-img").unwrap();
    let src = test_png(t.path(), "photo.png", 64, 48);
    for fmt in ["jpg", "webp", "tiff", "gif", "bmp"] {
        let ui = RecUi::default();
        let report = drop_on("convert", &format!("convert:{fmt}"), &[&src], &ui);
        assert_eq!(ok(&report).in_bytes, fs::metadata(&src).unwrap().len(), "before → after sizes");
        let out = one_output(&report);
        assert_eq!(out.extension().unwrap(), if fmt == "jpg" { "jpg" } else { fmt });
        let img = image::open(&out).unwrap_or_else(|e| panic!("{fmt}: {e}"));
        assert_eq!((img.width(), img.height()), (64, 48), "{fmt}");
        if fmt == "jpg" {
            let px = img.to_rgb8().get_pixel(0, 0).0;
            assert!(px.iter().all(|c| *c > 240), "transparent corner must be white, got {px:?}");
        }
        assert_eq!(ui.sounds.lock().unwrap().as_slice(), ["done"]);
    }
    // Same format is not offered.
    let items = wheel::items_for(&get_wheel_config(), "convert", &[src.to_string_lossy().to_string()]);
    assert!(!items.iter().any(|i| i.id == "convert:png"));
}

#[test]
fn e2e_image_to_avif_and_pdf() {
    if !have(Program::Ffmpeg) {
        return;
    }
    let t = TempDir::new("e2e-avif").unwrap();
    let src = test_png(t.path(), "pic.png", 4000, 3000);
    let avif = one_output(&drop_on("convert", "convert:avif", &[&src], &RecUi::default()));
    assert_eq!(probe(&avif, "stream=codec_name", Some("v:0")), "av1");

    let pdf = one_output(&drop_on("convert", "convert:pdf", &[&src], &RecUi::default()));
    let doc = lopdf::Document::load(&pdf).unwrap();
    let page = doc.get_pages().into_values().next().unwrap();
    let mb = doc.get_dictionary(page).unwrap().get(b"MediaBox").unwrap().as_array().unwrap().clone();
    let w = mb[2].as_float().unwrap();
    assert!((w - 841.89).abs() < 0.1, "landscape A4 expected, MediaBox width {w}");
    if have(Program::Pdftoppm) {
        let out = Command::new("pdftoppm").args(["-r", "20", "-png", "-singlefile"]).arg(&pdf).arg(t.path().join("render")).output().unwrap();
        assert!(out.status.success(), "the PDF must render");
    }
}

#[test]
fn e2e_exif_orientation_is_applied() {
    let t = TempDir::new("e2e-exif").unwrap();
    let src = rotated_jpeg(t.path(), 40, 20);
    let out = one_output(&drop_on("convert", "convert:png", &[&src], &RecUi::default()));
    let img = image::open(&out).unwrap();
    assert_eq!((img.width(), img.height()), (20, 40), "phone photo must come out upright");
    let rot = one_output(&drop_on("tools", "tool:image.rotate", &[&src], &RecUi::default()));
    let img = image::open(&rot).unwrap();
    assert_eq!((img.width(), img.height()), (40, 20), "rotate starts from the upright image");
}

#[test]
fn e2e_svg_input_via_ffmpeg() {
    if !have(Program::Ffmpeg) {
        return;
    }
    let t = TempDir::new("e2e-svg").unwrap();
    let svg = t.path().join("logo.svg");
    fs::write(&svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="20"><rect width="30" height="20" fill="#0a0"/></svg>"##).unwrap();
    let out = one_output(&drop_on("convert", "convert:png", &[&svg], &RecUi::default()));
    let img = image::open(&out).unwrap();
    assert_eq!((img.width(), img.height()), (30, 20));
}

#[test]
fn e2e_ocr_reports_a_clear_error_off_windows() {
    if cfg!(windows) {
        return;
    }
    let t = TempDir::new("e2e-ocr").unwrap();
    let src = test_png(t.path(), "scan.png", 30, 30);
    let r = drop_on("convert", "convert:txt", &[&src], &RecUi::default());
    let msg = r.final_msg.unwrap();
    assert_eq!(msg.state, "error");
    assert!(msg.detail.contains("Windows"), "{msg:?}");
}

#[test]
fn e2e_image_tools() {
    let t = TempDir::new("e2e-itools").unwrap();
    let big = test_png(t.path(), "big.png", 3000, 1600);
    let small = test_png(t.path(), "small.png", 300, 200);

    let dims = |p: &Path| {
        let i = image::open(p).unwrap();
        (i.width(), i.height())
    };
    let run = |id: &str, src: &Path| drop_on("tools", id, &[src], &RecUi::default());

    assert_eq!(dims(&one_output(&run("tool:image.half", &big))), (1500, 800));
    assert_eq!(dims(&one_output(&run("tool:image.fit", &big))), (1920, 1024));
    assert_eq!(dims(&one_output(&run("tool:image.square", &big))), (1600, 1600));
    assert_eq!(dims(&one_output(&run("tool:image.rotate", &small))), (200, 300));
    assert_eq!(dims(&one_output(&run("tool:image.flip", &small))), (300, 200));
    let gray = image::open(one_output(&run("tool:image.gray", &small))).unwrap().to_rgb8();
    assert!(gray.pixels().all(|p| p[0] == p[1] && p[1] == p[2]));

    // fit never upscales
    let r = run("tool:image.fit", &small);
    assert_eq!(r.final_msg.as_ref().unwrap().state, "info");
    assert!(r.outputs.is_empty());

    // compress: big image shrinks to max 2560 px
    let c = one_output(&run("tool:image.compress", &big));
    assert_eq!(dims(&c).0, 2560);
    assert!(fs::metadata(&c).unwrap().len() < fs::metadata(&big).unwrap().len());

    // trim: white border around a black box
    let mut bordered = image::RgbImage::from_pixel(100, 80, image::Rgb([255, 255, 255]));
    for y in 20..50 {
        for x in 10..70 {
            bordered.put_pixel(x, y, image::Rgb([0, 0, 0]));
        }
    }
    let b = t.path().join("shot.png");
    bordered.save(&b).unwrap();
    assert_eq!(dims(&one_output(&run("tool:image.trim", &b))), (60, 30));
    let r = run("tool:image.trim", &small);
    assert_eq!(r.final_msg.unwrap().state, "info");

    // the source files are untouched
    assert_eq!(dims(&big), (3000, 1600));
}

#[test]
fn e2e_images_to_one_pdf_and_crop() {
    let t = TempDir::new("e2e-multi").unwrap();
    let a = test_png(t.path(), "a.png", 200, 100);
    let b = test_png(t.path(), "b.png", 100, 200);
    let pdf = one_output(&drop_on("tools", "tool:image.pdf", &[&a, &b], &RecUi::default()));
    assert_eq!(pdfops::page_count(&pdf).unwrap(), 2);
    assert!(pdf.file_name().unwrap().to_string_lossy().contains("combined"));

    // crop: the editor answers with the right half
    let ui = RecUi::default();
    *ui.crop_answer.lock().unwrap() = Some(CropRect { x: 0.5, y: 0.0, w: 0.5, h: 1.0 });
    let r = drop_on("tools", "tool:image.crop", &[&a, &b], &ui);
    ok(&r);
    let req = ui.crop_requests.lock().unwrap()[0].clone();
    assert_eq!((req.width, req.height, req.count), (200, 100, 2));
    assert!(req.src.starts_with("data:image/jpeg;base64,"));
    let sizes: Vec<_> = r.outputs.iter().map(|p| image::open(p).unwrap()).map(|i| (i.width(), i.height())).collect();
    assert_eq!(sizes, vec![(100, 100), (50, 200)]);

    // cancelling the editor writes nothing
    let ui = RecUi::default();
    let r = drop_on("tools", "tool:image.crop", &[&a], &ui);
    assert!(r.outputs.is_empty());
    assert_eq!(r.final_msg.unwrap().label, "Crop cancelled");
}

// ---------------------------------------------------------------------------------------------
// Documents and PDF
// ---------------------------------------------------------------------------------------------

#[test]
fn e2e_office_conversion_never_overwrites() {
    if !have(Program::Soffice) {
        return;
    }
    let t = TempDir::new("e2e-office").unwrap();
    let txt = t.path().join("report.txt");
    fs::write(&txt, "Quarterly report\nRevenue: 42\n").unwrap();

    let docx = one_output(&drop_on("convert", "convert:docx", &[&txt], &RecUi::default()));
    assert_eq!(docx.file_name().unwrap(), "report.docx");

    // An unrelated report.pdf already exists: it must survive.
    let existing = t.path().join("report.pdf");
    fs::write(&existing, b"precious").unwrap();
    let pdf = one_output(&drop_on("convert", "convert:pdf", &[&docx], &RecUi::default()));
    assert_eq!(pdf.file_name().unwrap(), "report (1).pdf");
    assert_eq!(fs::read(&existing).unwrap(), b"precious");
    assert!(fs::read(&pdf).unwrap().starts_with(b"%PDF"));

    // docx -> docx is not offered; docx -> txt keeps the text
    let items = wheel::items_for(&get_wheel_config(), "convert", &[docx.to_string_lossy().to_string()]);
    assert!(!items.iter().any(|i| i.id == "convert:docx"));
    let back = one_output(&drop_on("convert", "convert:txt", &[&docx], &RecUi::default()));
    assert_eq!(back.file_name().unwrap(), "report (1).txt");
    assert!(fs::read_to_string(&back).unwrap().contains("Revenue: 42"));
}

#[test]
fn e2e_spreadsheet_roundtrip() {
    if !have(Program::Soffice) {
        return;
    }
    let t = TempDir::new("e2e-sheet").unwrap();
    let csv = t.path().join("data.csv");
    fs::write(&csv, "name,amount\nÄpfel,3\nBirnen,5\n").unwrap();
    let xlsx = one_output(&drop_on("convert", "convert:xlsx", &[&csv], &RecUi::default()));
    let back = one_output(&drop_on("convert", "convert:csv", &[&xlsx], &RecUi::default()));
    assert_eq!(back.file_name().unwrap(), "data (1).csv");
    let text = fs::read_to_string(&back).unwrap();
    assert!(text.contains("Äpfel") && text.contains("Birnen"), "{text}");
}

#[test]
fn e2e_pdf_conversions() {
    if !have(Program::Soffice) {
        return;
    }
    let t = TempDir::new("e2e-pdfconv").unwrap();
    let pdf = office_pdf(t.path(), "manual", 150);
    let pages = pdfops::page_count(&pdf).unwrap();
    assert!(pages >= 2, "fixture should have several pages, has {pages}");

    let txt = one_output(&drop_on("convert", "convert:txt", &[&pdf], &RecUi::default()));
    let text = fs::read_to_string(&txt).unwrap();
    assert!(text.contains("manual line 1 ") && text.contains("manual line 150 "), "{text}");
    // Without Poppler the LibreOffice route must give the same lines in order.
    let via_office = crate::converter::pdf_text_via_office(&pdf).unwrap();
    let pos = |s: &str| via_office.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(pos("manual line 1 ") < pos("manual line 2 ") && pos("manual line 99 ") < pos("manual line 150 "));

    let docx = one_output(&drop_on("convert", "convert:docx", &[&pdf], &RecUi::default()));
    assert!(fs::read(&docx).unwrap().starts_with(b"PK"));

    if have(Program::Pdftoppm) {
        let r = drop_on("convert", "convert:png", &[&pdf], &RecUi::default());
        ok(&r);
        assert_eq!(r.outputs.len(), pages);
        assert!(r.outputs[0].file_name().unwrap().to_string_lossy().ends_with("(page 1).png"));
        for p in &r.outputs {
            image::open(p).unwrap();
        }
    }
}

#[test]
fn e2e_pdf_tools() {
    if !have(Program::Soffice) {
        return;
    }
    let t = TempDir::new("e2e-pdftools").unwrap();
    let a = office_pdf(t.path(), "alpha", 150);
    let b = office_pdf(t.path(), "beta", 60);
    let (na, nb) = (pdfops::page_count(&a).unwrap(), pdfops::page_count(&b).unwrap());

    let merged = one_output(&drop_on("tools", "tool:pdf.merge", &[&a, &b], &RecUi::default()));
    assert_eq!(pdfops::page_count(&merged).unwrap(), na + nb);
    if have(Program::Pdftoppm) {
        // Every page must still render (inherited MediaBox/Resources kept).
        let out = Command::new("pdftotext").arg(&merged).arg("-").output();
        if let Ok(out) = out {
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(text.contains("alpha line 1") && text.contains("beta line 60"), "merged text missing");
        }
    }

    let r = drop_on("tools", "tool:pdf.split", &[&a], &RecUi::default());
    ok(&r);
    assert_eq!(r.outputs.len(), na);
    for p in &r.outputs {
        assert_eq!(pdfops::page_count(p).unwrap(), 1);
    }

    let r = drop_on("tools", "tool:pdf.compress", &[&a], &RecUi::default());
    let state = r.final_msg.as_ref().unwrap().state.clone();
    assert!(state == "done" || state == "info", "{:?}", r.final_msg);
    for p in &r.outputs {
        assert_eq!(pdfops::page_count(p).unwrap(), na);
        assert!(fs::metadata(p).unwrap().len() < fs::metadata(&a).unwrap().len());
    }

    if have(Program::Pdftoppm) {
        let r = drop_on("tools", "tool:pdf.images", &[&b], &RecUi::default());
        ok(&r);
        assert_eq!(r.outputs.len(), nb);
    }

    // single-page PDF can't be split; message instead of a crash
    let one = &drop_on("tools", "tool:pdf.split", &[&a], &RecUi::default()).outputs[0];
    let r = drop_on("tools", "tool:pdf.split", &[one], &RecUi::default());
    assert_eq!(r.final_msg.unwrap().state, "error");
}

// ---------------------------------------------------------------------------------------------
// Audio / video
// ---------------------------------------------------------------------------------------------

#[test]
fn e2e_audio_conversions_and_tools() {
    if !have(Program::Ffmpeg) || !have(Program::Ffprobe) {
        return;
    }
    let t = TempDir::new("e2e-audio").unwrap();
    let wav = test_wav(t.path());
    let len = duration(&wav);
    assert!((len - 3.0).abs() < 0.1, "fixture length {len}");

    for (fmt, codec) in [("mp3", "mp3"), ("m4a", "aac"), ("flac", "flac"), ("ogg", "vorbis"), ("opus", "opus"), ("aiff", "pcm_s16be")] {
        let out = one_output(&drop_on("convert", &format!("convert:{fmt}"), &[&wav], &RecUi::default()));
        assert_eq!(probe(&out, "stream=codec_name", Some("a:0")), codec, "{fmt}");
    }

    let run = |id: &str| one_output(&drop_on("tools", id, &[&wav], &RecUi::default()));
    let trimmed = duration(&run("tool:audio.trim"));
    assert!(trimmed < 2.4 && trimmed > 1.6, "trim should drop ~1 s of silence, got {trimmed}");
    let fast = duration(&run("tool:audio.fast"));
    assert!((fast - len / 2.0).abs() < 0.15, "2x speed: {fast}");
    let mono = run("tool:audio.mono");
    assert_eq!(probe(&mono, "stream=channels", Some("a:0")), "1");
    let faded = run("tool:audio.fade");
    assert!((duration(&faded) - len).abs() < 0.1);
}

#[test]
fn e2e_video_conversions_and_tools() {
    if !have(Program::Ffmpeg) || !have(Program::Ffprobe) {
        return;
    }
    let t = TempDir::new("e2e-video").unwrap();
    let mp4 = test_mp4(t.path());

    let webm = one_output(&drop_on("convert", "convert:webm", &[&mp4], &RecUi::default()));
    assert_eq!(probe(&webm, "stream=codec_name", Some("v:0")), "vp9");
    let mkv = one_output(&drop_on("convert", "convert:mkv", &[&mp4], &RecUi::default()));
    assert_eq!(probe(&mkv, "stream=codec_name", Some("v:0")), "h264");
    let gif = one_output(&drop_on("convert", "convert:gif", &[&mp4], &RecUi::default()));
    assert_eq!(probe(&gif, "stream=width", Some("v:0")), "480");
    let mp3 = one_output(&drop_on("convert", "convert:mp3", &[&mp4], &RecUi::default()));
    assert_eq!(probe(&mp3, "stream=codec_name", Some("a:0")), "mp3");

    let run = |id: &str, src: &Path| one_output(&drop_on("tools", id, &[src], &RecUi::default()));
    let muted = run("tool:video.mute", &mp4);
    assert_eq!(audio_streams(&muted), 0);
    let audio = run("tool:video.audio", &mp4);
    assert_eq!(audio.extension().unwrap(), "mp3");
    let fast = run("tool:video.fast", &mp4);
    assert!((duration(&fast) - 1.0).abs() < 0.25, "2x: {}", duration(&fast));
    let fast_muted = run("tool:video.fast", &muted); // no audio track must not fail
    assert_eq!(audio_streams(&fast_muted), 0);
    let small = run("tool:video.half", &mp4);
    assert_eq!(probe(&small, "stream=height", Some("v:0")), "720");
    let rev = run("tool:video.reverse", &mp4);
    assert!((duration(&rev) - 2.0).abs() < 0.3);
}

// ---------------------------------------------------------------------------------------------
// Actions, errors, batches
// ---------------------------------------------------------------------------------------------

#[test]
fn e2e_actions() {
    let t = TempDir::new("e2e-actions").unwrap();
    let f = t.path().join("a file.txt");
    fs::write(&f, "x").unwrap();
    let folder = t.path().join("Folder");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("inner.txt"), "y").unwrap();
    let dest = t.path().join("dest");
    fs::create_dir(&dest).unwrap();

    let ui = RecUi::default();
    ok(&drop_on("actions", "action:copy-path", &[&f, &folder], &ui));
    assert_eq!(ui.clipboard.lock().unwrap()[0], format!("{}\r\n{}", f.display(), folder.display()));

    // Folders show actions but no conversions.
    let items = wheel::items_for(&get_wheel_config(), "convert", &[folder.to_string_lossy().to_string()]);
    assert!(items.is_empty());

    let ui = RecUi::default();
    *ui.folder.lock().unwrap() = Some(dest.clone());
    let r = drop_on("actions", "action:copy-to", &[&f, &folder], &ui);
    ok(&r);
    assert!(dest.join("a file.txt").exists() && dest.join("Folder").join("inner.txt").exists());
    assert!(f.exists() && folder.exists());
    assert!(r.final_msg.unwrap().detail.starts_with("2 copied"));

    let ui = RecUi::default();
    *ui.folder.lock().unwrap() = Some(dest.clone());
    ok(&drop_on("actions", "action:move-to", &[&f, &folder], &ui));
    assert!(!f.exists() && !folder.exists());
    assert!(dest.join("a file (1).txt").exists() && dest.join("Folder (1)").join("inner.txt").exists());

    // Cancelled picker
    let g = t.path().join("g.txt");
    fs::write(&g, "g").unwrap();
    let r = drop_on("actions", "action:move-to", &[&g], &RecUi::default());
    assert_eq!(r.final_msg.unwrap().state, "info");
    assert!(g.exists());
}

#[test]
fn e2e_failures_are_reported_honestly() {
    let t = TempDir::new("e2e-fail").unwrap();
    let good = test_png(t.path(), "good.png", 10, 10);
    let bad = t.path().join("broken.png");
    fs::write(&bad, b"not an image").unwrap();

    // all failed
    let ui = RecUi::default();
    let r = drop_on("convert", "convert:jpg", &[&bad], &ui);
    let msg = r.final_msg.unwrap();
    assert_eq!(msg.state, "error");
    assert!(msg.label.ends_with("Failed"), "{msg:?}");
    assert!(!msg.detail.is_empty());
    assert!(!msg.clickable);
    assert_eq!(ui.sounds.lock().unwrap().as_slice(), ["error"]);

    // partly failed
    let ui = RecUi::default();
    let r = drop_on("convert", "convert:jpg", &[&good, &bad], &ui);
    let msg = r.final_msg.unwrap();
    assert_eq!(msg.label, "Convert to JPG · 1 of 2 failed");
    assert!(msg.detail.starts_with("broken.png:"), "{msg:?}");
    assert_eq!(r.outputs.len(), 1);
    assert!(msg.clickable);
    // progress went 1 of 2, 2 of 2
    let hud = ui.hud.lock().unwrap();
    assert_eq!(hud.iter().map(|m| m.index).collect::<Vec<_>>(), vec![1, 2]);

    // file deleted between drag and drop
    let gone = t.path().join("gone.png");
    let r = run_job("convert:jpg", &[gone.to_string_lossy().to_string()], &RecUi::default());
    assert_eq!(r.final_msg.unwrap().detail, "The file no longer exists");
}

#[test]
fn e2e_unknown_types() {
    let t = TempDir::new("e2e-unknown").unwrap();
    // Binary files of unknown type get no conversions at all...
    let zip = t.path().join("archive.zip");
    fs::write(&zip, b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0").unwrap();
    let zip_s = zip.to_string_lossy().to_string();
    assert!(wheel::items_for(&get_wheel_config(), "convert", std::slice::from_ref(&zip_s)).is_empty());
    let r = run_job("convert:pdf", &[zip_s], &RecUi::default());
    assert_eq!(r.final_msg.unwrap().detail, "Shiftwheel can't convert .zip files");
    assert_eq!(fs::read_dir(t.path()).unwrap().count(), 1, "nothing written");

    // ...while text files with other extensions convert like documents.
    if !have(Program::Soffice) {
        return;
    }
    let json = t.path().join("config.json");
    fs::write(&json, "{\n  \"name\": \"Shiftwheel\"\n}\n").unwrap();
    let pdf = one_output(&drop_on("convert", "convert:pdf", &[&json], &RecUi::default()));
    assert_eq!(pdf.file_name().unwrap(), "config.pdf");
    let txt = one_output(&drop_on("convert", "convert:txt", &[&pdf], &RecUi::default()));
    assert!(fs::read_to_string(txt).unwrap().contains("Shiftwheel"));
    let md = t.path().join("notes.md");
    fs::write(&md, "# Title\n\nSome *notes*.\n").unwrap();
    one_output(&drop_on("convert", "convert:pdf", &[&md], &RecUi::default()));
}

// ---------------------------------------------------------------------------------------------
// IPC: the commands and events bridge.js uses exist on the Rust side, with the same shapes
// ---------------------------------------------------------------------------------------------

fn quoted_after<'a>(src: &'a str, marker: &str) -> Vec<&'a str> {
    src.match_indices(marker)
        .filter_map(|(i, _)| {
            let rest = src[i + marker.len()..].trim_start();
            let q = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
            let end = rest[1..].find(q)?;
            Some(&rest[1..1 + end])
        })
        .collect()
}

#[test]
fn ipc_names_match_between_bridge_and_backend() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bridge = fs::read_to_string(root.join("../ui/bridge.js")).unwrap();
    let rust: String = ["lib.rs", "tray.rs"].iter().map(|f| fs::read_to_string(root.join("src").join(f)).unwrap()).collect();

    let invoked: std::collections::BTreeSet<_> = quoted_after(&bridge, "invoke(").into_iter().collect();
    let listened: std::collections::BTreeSet<_> = quoted_after(&bridge, "on(").into_iter().collect();
    let emitted: std::collections::BTreeSet<_> = quoted_after(&rust, ".emit(").into_iter().collect();
    let handler = &rust[rust.find("generate_handler![").unwrap()..];
    let registered: Vec<&str> = handler["generate_handler![".len()..handler.find(']').unwrap()].split(',').map(str::trim).collect();

    for cmd in &invoked {
        assert!(registered.contains(cmd), "bridge invokes `{cmd}` but it is not registered: {registered:?}");
    }
    for ev in &listened {
        assert!(emitted.contains(ev), "bridge listens to `{ev}` but the backend never emits it");
    }
    for ev in &emitted {
        assert!(listened.contains(ev), "backend emits `{ev}` but nothing listens to it");
    }
    assert!(invoked.len() >= 5 && listened.len() >= 10, "{invoked:?} {listened:?}");
}

#[test]
fn ipc_commands_accept_what_bridge_sends() {
    use serde_json::json;
    use tauri::Manager;
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};

    let app = mock_builder()
        .manage(crate::new_state())
        .invoke_handler(crate::commands())
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "crop", Default::default()).build().unwrap();
    let call = |cmd: &str, body: serde_json::Value| {
        get_ipc_response(
            &webview,
            tauri::webview::InvokeRequest {
                cmd: cmd.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: if cfg!(windows) { "http://tauri.localhost" } else { "tauri://localhost" }.parse().unwrap(),
                body: tauri::ipc::InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|b| b.deserialize::<serde_json::Value>().unwrap())
    };

    // overlay: invoke('get_settings'), invoke('overlay_revealed', { ok })
    let settings = call("get_settings", json!({})).unwrap();
    assert!(settings.get("sound").is_some_and(|v| v.is_boolean()), "{settings}");
    call("overlay_revealed", json!({ "ok": true })).unwrap();
    call("overlay_revealed", json!({ "ok": false })).unwrap();

    // crop: a pending request is handed to the page, and the answer reaches the waiting job
    let st = app.state::<crate::AppState>();
    *st.crop_request.lock().unwrap() =
        Some(CropRequest { src: "data:".into(), width: 4, height: 2, name: "a.png".into(), count: 1 });
    let req = call("get_crop_request", json!({})).unwrap();
    assert_eq!(req["name"], "a.png");

    let (tx, rx) = std::sync::mpsc::channel();
    *st.crop_reply.lock().unwrap() = Some(tx);
    call("crop_done", json!({ "rect": { "x": 0.25, "y": 0.0, "w": 0.5, "h": 1.0 } })).unwrap();
    assert_eq!(rx.try_recv().unwrap(), Some(CropRect { x: 0.25, y: 0.0, w: 0.5, h: 1.0 }));

    let (tx, rx) = std::sync::mpsc::channel();
    *st.crop_reply.lock().unwrap() = Some(tx);
    call("crop_done", json!({ "rect": null })).unwrap();
    assert_eq!(rx.try_recv().unwrap(), None);

    // hud: invoke('hud_reveal') with nothing to reveal is harmless
    call("hud_reveal", json!({})).unwrap();

    // first run: nothing pending -> null; pending -> the card's data; "Got it" stores it.
    // Settings go to a scratch folder; autostart is passed back unchanged so the registry is untouched.
    let scratch = TempDir::new("ipc-settings").unwrap();
    unsafe { std::env::set_var("APPDATA", scratch.path()) };
    assert!(call("get_first_run", json!({})).unwrap().is_null());
    st.first_run_pending.store(true, std::sync::atomic::Ordering::SeqCst);
    let card = call("get_first_run", json!({})).unwrap();
    let autostart = card["autostart"].as_bool().expect("autostart flag");
    call("first_run_done", json!({ "autostart": autostart })).unwrap();
    assert!(crate::settings::load_settings().onboarded);
    assert!(call("get_first_run", json!({})).unwrap().is_null());

    assert!(call("overlay_drop", json!({})).is_err(), "removed command must not exist");
}

#[test]
fn e2e_parallel_batch_keeps_names_and_order() {
    let t = TempDir::new("e2e-par").unwrap();
    let mut files = Vec::new();
    for i in 0..6 {
        files.push(test_png(t.path(), &format!("img{i}.png"), 300 + i * 10, 200));
    }
    // Same stem, different format: both become "same.webp" -> one must get "(1)".
    let same_png = test_png(t.path(), "same.png", 50, 40);
    let same_bmp = t.path().join("same.bmp");
    image::open(&same_png).unwrap().save(&same_bmp).unwrap();
    files.push(same_png);
    files.push(same_bmp);
    let broken = t.path().join("broken.png");
    fs::write(&broken, b"nope").unwrap();
    files.insert(3, broken);

    let refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
    let ui = RecUi::default();
    let r = drop_on("convert", "convert:webp", &refs, &ui);
    assert_eq!(r.outputs.len(), 8, "{:?}", r.errors);
    assert_eq!(r.errors.len(), 1);
    assert!(r.errors[0].starts_with("broken.png:"), "{:?}", r.errors);
    let names: Vec<String> = r.outputs.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
    assert_eq!(&names[..6], ["img0.webp", "img1.webp", "img2.webp", "img3.webp", "img4.webp", "img5.webp"], "selection order kept");
    let mut sorted = names.clone();
    sorted.sort();
    assert!(sorted.contains(&"same.webp".to_string()) && sorted.contains(&"same (1).webp".to_string()), "{names:?}");
    for (out, src) in r.outputs.iter().zip(files.iter().filter(|p| !p.ends_with("broken.png"))) {
        let a = image::open(out).unwrap();
        let b = image::open(src).unwrap();
        assert_eq!((a.width(), a.height()), (b.width(), b.height()), "{} vs {}", out.display(), src.display());
    }
    // Progress went from 1 to 9 without going backwards.
    let idx: Vec<usize> = ui.hud.lock().unwrap().iter().map(|m| m.index).collect();
    assert!(idx.windows(2).all(|w| w[0] <= w[1]) && *idx.last().unwrap() == 9, "{idx:?}");
    assert_eq!(r.final_msg.unwrap().label, "Convert to WEBP · 1 of 9 failed");
}

/// `cargo test --release --lib bench_batch -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_batch_sequential_vs_parallel() {
    let t = TempDir::new("bench-batch").unwrap();
    let files: Vec<PathBuf> = (0..8).map(|i| test_png(t.path(), &format!("p{i}.png"), 4000, 3000)).collect();
    let paths: Vec<String> = files.iter().map(|p| p.to_string_lossy().to_string()).collect();
    let start = std::time::Instant::now();
    for p in &files {
        crate::converter::convert_file(p, "jpg").unwrap();
    }
    let seq = start.elapsed();
    let start = std::time::Instant::now();
    let r = run_job("convert:jpg", &paths, &RecUi::default());
    let par = start.elapsed();
    assert_eq!(r.outputs.len(), 8);
    eprintln!("BENCH 8 x 12 MP PNG->JPG: sequential {seq:?} | job (parallel) {par:?} | cores {:?}", std::thread::available_parallelism());
}
