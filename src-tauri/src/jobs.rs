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
        actions::pick_folder()
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
    crate::config::get_wheel_config()
        .tools
        .values()
        .flatten()
        .find(|i| i.id == format!("tool:{tool}"))
        .map(|i| i.label.clone())
        .unwrap_or_else(|| tool.to_string())
}

fn busy(name: &str, label: &str, chip: &str, index: usize, total: usize, percent: Option<f64>) -> HudMsg {
    HudMsg {
        name: name.into(),
        label: label.into(),
        chip: chip.into(),
        index,
        total,
        percent,
        state: "busy".into(),
        detail: String::new(),
        clickable: false,
    }
}

/// Build the final card from what happened and send it, with a sound.
fn conclude(ui: &dyn Ui, mut report: Report, name: &str, label: &str, chip: &str, total: usize) -> Report {
    let n_out = report.outputs.len();
    let n_err = report.errors.len();
    let mut msg = busy(name, label, chip, total, total, Some(1.0));
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
        msg.detail = match n_out {
            0 => String::new(),
            1 => format!("Saved as {}", file_name(&report.outputs[0].to_string_lossy())),
            n => format!("{n} files saved"),
        };
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

/// Run `f` on each file with progress updates.
fn per_file(
    ui: &dyn Ui,
    paths: &[String],
    label: &str,
    chip: &str,
    f: impl Fn(&Path) -> Result<Outcome, String>,
) -> Report {
    let total = paths.len();
    let mut report = Report::default();
    for (i, p) in paths.iter().enumerate() {
        let name = file_name(p);
        ui.hud(&busy(&name, label, chip, i + 1, total, if total > 1 { Some(i as f64 / total as f64) } else { None }));
        match f(Path::new(p)) {
            Ok(Outcome::Files(files)) => report.outputs.extend(files),
            Ok(Outcome::Skipped(why)) => report.skipped.push(why),
            Err(e) => {
                log::error!("{label} failed for {p}: {e}");
                report.errors.push(if total > 1 { format!("{name}: {e}") } else { e });
            }
        }
    }
    conclude(ui, report, &selection_name(paths), label, chip, total)
}

fn single(ui: &dyn Ui, paths: &[String], label: &str, chip: &str, r: Result<Outcome, String>) -> Report {
    let mut report = Report::default();
    match r {
        Ok(Outcome::Files(f)) => report.outputs = f,
        Ok(Outcome::Skipped(s)) => report.skipped.push(s),
        Err(e) => report.errors.push(e),
    }
    conclude(ui, report, &selection_name(paths), label, chip, 1)
}

pub fn run_job(id: &str, paths: &[String], ui: &dyn Ui) -> Report {
    let Some((kind, value)) = id.split_once(':') else {
        return Report::default();
    };
    if paths.is_empty() {
        return Report::default();
    }
    let name = selection_name(paths);

    match kind {
        "convert" => {
            let fmt = value.to_uppercase();
            per_file(ui, paths, &format!("Convert to {fmt}"), &fmt, |p| {
                converter::convert_file(p, value).map(Outcome::Files)
            })
        }
        "tool" => {
            let label = tool_label(value);
            let bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
            match value {
                "image.pdf" => {
                    ui.hud(&busy(&name, &label, "TOOL", 1, 1, None));
                    single(ui, paths, &label, "TOOL", converter::images_to_combined_pdf(&bufs))
                }
                "pdf.merge" => {
                    ui.hud(&busy(&name, &label, "TOOL", 1, 1, None));
                    single(ui, paths, &label, "TOOL", converter::merge_pdfs(&bufs))
                }
                "image.crop" => crop_job(ui, paths, &label),
                _ => per_file(ui, paths, &label, "TOOL", |p| converter::run_tool(value, p)),
            }
        }
        "action" => action_job(ui, value, paths),
        _ => Report::default(),
    }
}

fn crop_job(ui: &dyn Ui, paths: &[String], label: &str) -> Report {
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
        Err(e) => {
            let report = Report { errors: vec![e], ..Default::default() };
            return conclude(ui, report, &selection_name(paths), label, "TOOL", 1);
        }
    };
    match ui.ask_crop(req) {
        None => {
            let report = Report { skipped: vec!["Crop cancelled".into()], ..Default::default() };
            conclude(ui, report, &selection_name(paths), label, "TOOL", paths.len())
        }
        Some(rect) => per_file(ui, paths, label, "TOOL", |p| converter::crop_image(p, rect)),
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
    let name = selection_name(paths);
    let mut report = Report::default();
    let mut detail = String::new();
    let mut reveal: Option<PathBuf> = None;

    let simple = |r: Result<(), String>, report: &mut Report| {
        if let Err(e) = r {
            report.errors.push(e);
        }
    };

    match action {
        "copy-path" => simple(ui.copy_text(&paths.join("\r\n")), &mut report),
        "copy" => simple(ui.copy_files(paths), &mut report),
        "terminal" => simple(ui.open_terminal(paths), &mut report),
        "reveal" => simple(ui.reveal(&paths[0]), &mut report),
        "copy-to" | "move-to" => {
            ui.hud(&busy(&name, &format!("{label}…"), "ACTION", 1, 1, None));
            match ui.pick_folder() {
                None => report.skipped.push(format!("{label} cancelled")),
                Some(dir) => {
                    let t = actions::transfer_files(paths, &dir, action == "move-to");
                    report.errors = t.errors;
                    detail = format!("{} {} to {}", t.done.len(), if action == "move-to" { "moved" } else { "copied" }, dir.display());
                    reveal = t.done.last().cloned();
                }
            }
        }
        _ => report.errors.push(format!("Unknown action: {action}")),
    }

    // Actions don't create files in the usual sense; build the card here.
    let mut msg = busy(&name, label, "ACTION", 1, 1, Some(1.0));
    if !report.skipped.is_empty() && report.errors.is_empty() {
        msg.state = "info".into();
        msg.label = report.skipped[0].clone();
    } else if report.errors.is_empty() {
        msg.state = "done".into();
        msg.label = format!("{label} · Done");
        msg.detail = detail;
    } else {
        msg.state = "error".into();
        msg.label = format!("{label} · Failed");
        msg.detail = report.errors[0].clone();
        if report.errors.len() > 1 {
            msg.detail.push_str(&format!(" (+{} more)", report.errors.len() - 1));
        }
    }
    msg.clickable = reveal.is_some();
    ui.sound(if msg.state == "error" { "error" } else { "done" });
    ui.finish(&msg, reveal.clone());
    if let Some(r) = reveal {
        report.outputs.push(r);
    }
    report.final_msg = Some(msg);
    report
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
