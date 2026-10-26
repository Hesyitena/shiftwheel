//! Runs wheel selections (convert / tool / action) one at a time and reports progress.

use crate::actions;
use crate::converter::{self, CropRect, Outcome};
use crate::imaging;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

/// What the progress card shows.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HudMsg {
    pub name: String,
    pub label: String,
    pub chip: String,
    pub index: usize,
    pub total: usize,
    /// 0..1, or None for "working, progress unknown".
    pub percent: Option<f64>,
    /// "busy", "done", "info" (nothing to do / cancelled) or "error".
    pub state: String,
    /// Second line: the saved file name or the error message.
    pub detail: String,
    /// Clicking the card reveals the result in Explorer.
    pub clickable: bool,
}

/// Data for the crop editor.
#[derive(Debug, Clone, Serialize)]
pub struct CropRequest {
    pub src: String,
    pub width: u32,
    pub height: u32,
    pub name: String,
    pub count: usize,
}

/// Everything a job needs from the outside world. The app implements it with windows and Win32
/// calls; tests implement it with recorders.
pub trait Ui: Send + Sync + 'static {
    fn hud(&self, msg: &HudMsg);
    /// Final state of the card; `reveal` is what a click on it should highlight.
    fn finish(&self, msg: &HudMsg, reveal: Option<PathBuf>);
    fn sound(&self, name: &str);
    fn ask_crop(&self, req: CropRequest) -> Option<CropRect>;

    fn pick_folder(&self) -> Option<PathBuf> {
        actions::pick_folder(None)
    }
    fn copy_text(&self, text: &str) -> Result<(), String> {
        actions::copy_text_to_clipboard(text)
    }
    fn copy_files(&self, paths: &[String]) -> Result<(), String> {
        actions::copy_files_to_clipboard(paths)
    }
    fn open_terminal(&self, paths: &[String]) -> Result<(), String> {
        actions::open_terminal(paths)
    }
    fn reveal(&self, path: &str) -> Result<(), String> {
        actions::reveal_in_explorer(path)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn known_tool(tool: &str) -> bool {
    matches!(
        tool,
        "image.compress" | "image.half" | "image.fit" | "image.square" | "image.crop" | "image.rotate" | "image.flip"
            | "image.gray" | "image.trim" | "image.pdf" | "pdf.compress" | "pdf.split" | "pdf.images" | "pdf.merge"
            | "audio.trim" | "audio.fade" | "audio.fast" | "audio.mono" | "video.mute" | "video.audio" | "video.fast"
            | "video.half" | "video.reverse"
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn known_action(action: &str) -> bool {
    matches!(action, "copy-path" | "copy" | "copy-to" | "move-to" | "terminal" | "reveal")
}

/// Summary of a finished job (returned for tests and logging).
#[derive(Debug, Default)]
pub struct Report {
    pub outputs: Vec<PathBuf>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
    pub final_msg: Option<HudMsg>,
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string())
}

fn selection_name(paths: &[String]) -> String {
    match paths {
        [] => "Selection".into(),
        [one] => file_name(one),
        [first, rest @ ..] => format!("{} +{}", file_name(first), rest.len()),
    }
}

fn tool_label(tool: &str) -> String {
    crate::config::wheel_config()
        .tools
        .values()
        .flatten()
        .find(|i| i.id.strip_prefix("tool:") == Some(tool))
        .map(|i| i.label.clone())
        .unwrap_or_else(|| tool.to_string())
}

/// What a job's progress card is about.
struct Card<'a> {
    name: String,
    label: &'a str,
    chip: &'a str,
    total: usize,
}

impl Card<'_> {
    fn msg(&self, name: &str, index: usize, percent: Option<f64>) -> HudMsg {
        HudMsg {
            name: name.into(),
            label: self.label.into(),
            chip: self.chip.into(),
            index,
            total: self.total,
            percent,
            state: "busy".into(),
            detail: String::new(),
            clickable: false,
        }
    }

    fn busy(&self, ui: &dyn Ui) {
        ui.hud(&self.msg(&self.name, 1, None));
    }
}

/// Build the final card from what happened and send it, with a sound. `done_detail` replaces
/// the default "Saved as …" line on success.
fn conclude(ui: &dyn Ui, mut report: Report, card: &Card, done_detail: Option<String>) -> Report {
    let (n_out, n_err, total, label) = (report.outputs.len(), report.errors.len(), card.total, card.label);
    let mut msg = card.msg(&card.name, total, Some(1.0));
    let first_err = || {
        let more = if n_err > 1 { format!(" (+{} more)", n_err - 1) } else { String::new() };
        format!("{}{more}", report.errors[0])
    };

    if n_err == 0 && n_out == 0 && !report.skipped.is_empty() {
        msg.state = "info".into();
        msg.label = report.skipped[0].clone();
    } else if n_err == 0 {
        msg.state = "done".into();
        msg.label = format!("{label} · Done");
        msg.detail = done_detail.unwrap_or_else(|| match n_out {
            0 => String::new(),
            1 => format!("Saved as {}", file_name(&report.outputs[0].to_string_lossy())),
            n => format!("{n} files saved"),
        });
    } else if n_out == 0 {
        msg.state = "error".into();
        msg.label = format!("{label} · Failed");
        msg.detail = first_err();
    } else {
        msg.state = "error".into();
        msg.label = format!("{label} · {} of {total} failed", n_err.min(total));
        msg.detail = first_err();
    }
    msg.clickable = n_out > 0;

    ui.sound(if n_err > 0 && n_out == 0 { "error" } else { "done" });
    ui.finish(&msg, report.outputs.last().cloned());
    report.final_msg = Some(msg);
    report
}

fn record(report: &mut Report, r: Result<Outcome, String>, name: &str, many: bool, label: &str) {
    match r {
        Ok(Outcome::Files(files)) => report.outputs.extend(files),
        Ok(Outcome::Skipped(why)) => report.skipped.push(why),
        Err(e) => {
            log::error!("{label} failed for {name}: {e}");
            report.errors.push(if many { format!("{name}: {e}") } else { e });
        }
    }
}

/// Most images in flight at once: each full-size decode can take a few hundred MB.
const MAX_PARALLEL: usize = 4;

/// Run `f` on each file with progress updates. With `parallel`, files are processed on several
/// threads (pure-Rust image work); results and errors keep the selection order.
fn per_file(
    ui: &dyn Ui,
    paths: &[String],
    card: &Card,
    parallel: bool,
    f: impl Fn(&Path) -> Result<Outcome, String> + Sync,
) -> Report {
    let total = paths.len();
    let many = total > 1;
    let mut report = Report::default();
    let workers = if parallel {
        std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_PARALLEL).min(total)
    } else {
        1
    };

    if workers <= 1 {
        for (i, p) in paths.iter().enumerate() {
            let name = file_name(p);
            ui.hud(&card.msg(&name, i + 1, many.then(|| i as f64 / total as f64)));
            record(&mut report, f(Path::new(p)), &name, many, card.label);
        }
    } else {
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let results: Mutex<Vec<Option<Result<Outcome, String>>>> = Mutex::new((0..total).map(|_| None).collect());
        ui.hud(&card.msg(&file_name(&paths[0]), 1, Some(0.0)));
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        if i >= total {
                            break;
                        }
                        let r = f(Path::new(&paths[i]));
                        results.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(r);
                        let d = done.fetch_add(1, Ordering::SeqCst) + 1;
                        if d < total {
                            ui.hud(&card.msg(&file_name(&paths[i]), d + 1, Some(d as f64 / total as f64)));
                        }
                    }
                });
            }
        });
        let results = results.into_inner().unwrap_or_else(|e| e.into_inner());
        for (p, r) in paths.iter().zip(results) {
            record(&mut report, r.unwrap_or_else(|| Err("Not processed".into())), &file_name(p), many, card.label);
        }
    }
    conclude(ui, report, card, None)
}

fn single(ui: &dyn Ui, card: &Card, r: Result<Outcome, String>) -> Report {
    let mut report = Report::default();
    record(&mut report, r, &card.name, false, card.label);
    conclude(ui, report, card, None)
}

/// Image work the `image` crate does in-process: safe and worthwhile to run in parallel.
/// (FFmpeg and LibreOffice jobs stay sequential; they use several cores themselves.)
fn parallel_image_job(paths: &[String], target: Option<&str>) -> bool {
    let images = paths.iter().all(|p| converter::category_of(p) == "image");
    images && target.is_none_or(|t| imgcore::is_native(t) || t == "pdf")
}

pub fn run_job(id: &str, paths: &[String], ui: &dyn Ui) -> Report {
    let Some((kind, value)) = id.split_once(':') else {
        return Report::default();
    };
    if paths.is_empty() {
        return Report::default();
    }
    let name = selection_name(paths);
    let total = paths.len();

    match kind {
        "convert" => {
            let fmt = value.to_uppercase();
            let label = format!("Convert to {fmt}");
            let card = Card { name, label: &label, chip: &fmt, total };
            per_file(ui, paths, &card, parallel_image_job(paths, Some(value)), |p| {
                converter::convert_file(p, value).map(Outcome::Files)
            })
        }
        "tool" => {
            let label = tool_label(value);
            let bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
            match value {
                "image.pdf" | "pdf.merge" => {
                    let card = Card { name, label: &label, chip: "TOOL", total: 1 };
                    card.busy(ui);
                    let r = if value == "image.pdf" { converter::images_to_combined_pdf(&bufs) } else { converter::merge_pdfs(&bufs) };
                    single(ui, &card, r)
                }
                "image.crop" => crop_job(ui, paths, &Card { name, label: &label, chip: "TOOL", total }),
                _ => {
                    let card = Card { name, label: &label, chip: "TOOL", total };
                    let parallel = value.starts_with("image.") && parallel_image_job(paths, None);
                    per_file(ui, paths, &card, parallel, |p| converter::run_tool(value, p))
                }
            }
        }
        "action" => action_job(ui, value, paths),
        _ => Report::default(),
    }
}

fn crop_job(ui: &dyn Ui, paths: &[String], card: &Card) -> Report {
    let first = Path::new(&paths[0]);
    let prepared = imaging::load_image(first).and_then(|img| {
        Ok(CropRequest {
            src: imaging::preview_data_url(&img, 1600)?,
            width: img.width(),
            height: img.height(),
            name: file_name(&paths[0]),
            count: paths.len(),
        })
    });
    let req = match prepared {
        Ok(r) => r,
        Err(e) => return conclude(ui, Report { errors: vec![e], ..Default::default() }, card, None),
    };
    match ui.ask_crop(req) {
        None => conclude(ui, Report { skipped: vec!["Crop cancelled".into()], ..Default::default() }, card, None),
        Some(rect) => per_file(ui, paths, card, parallel_image_job(paths, None), |p| converter::crop_image(p, rect)),
    }
}

fn action_job(ui: &dyn Ui, action: &str, paths: &[String]) -> Report {
    let label = match action {
        "copy-path" => "Copy path",
        "copy" => "Copy",
        "copy-to" => "Copy to folder",
        "move-to" => "Move to folder",
        "terminal" => "Terminal",
        "reveal" => "Explorer",
        _ => "Action",
    };
    let card = Card { name: selection_name(paths), label, chip: "ACTION", total: paths.len() };
    let mut report = Report::default();
    let mut detail = None;

    let result = match action {
        "copy-path" => ui.copy_text(&paths.join("\r\n")),
        "copy" => ui.copy_files(paths),
        "terminal" => ui.open_terminal(paths),
        "reveal" => ui.reveal(&paths[0]),
        "copy-to" | "move-to" => {
            ui.hud(&card.msg(&card.name, 1, None));
            match ui.pick_folder() {
                None => report.skipped.push(format!("{label} cancelled")),
                Some(dir) => {
                    let moving = action == "move-to";
                    let t = actions::transfer_files(paths, &dir, moving);
                    report.errors = t.errors;
                    detail = Some(format!("{} {} to {}", t.done.len(), if moving { "moved" } else { "copied" }, dir.display()));
                    // Clicking the card shows the copied/moved items.
                    report.outputs = t.done;
                }
            }
            Ok(())
        }
        _ => Err(format!("Unknown action: {action}")),
    };
    if let Err(e) = result {
        report.errors.push(e);
    }
    conclude(ui, report, &card, detail)
}

pub struct Job {
    pub id: String,
    pub paths: Vec<String>,
}

/// One worker thread, so jobs never fight over the progress card.
pub struct JobQueue {
    tx: Sender<Job>,
}

impl JobQueue {
    pub fn start(ui: Arc<dyn Ui>) -> Self {
        let (tx, rx) = channel::<Job>();
        std::thread::Builder::new()
            .name("shiftwheel-jobs".into())
            .spawn(move || {
                for job in rx {
                    let report = run_job(&job.id, &job.paths, &*ui);
                    log::info!("{} finished: {} outputs, {} errors", job.id, report.outputs.len(), report.errors.len());
                }
            })
            .expect("job thread");
        Self { tx }
    }

    pub fn push(&self, job: Job) {
        let _ = self.tx.send(job);
    }
}
