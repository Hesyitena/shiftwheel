//! Finding and running helper programs (ffmpeg, LibreOffice, pdftoppm, PowerShell).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Keeps console programs from flashing a terminal window (Windows `CREATE_NO_WINDOW`).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A helper program Shiftwheel can call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Program {
    Ffmpeg,
    Ffprobe,
    Soffice,
    Pdftoppm,
    Pdftotext,
}

impl Program {
    fn exe_name(self) -> &'static str {
        match self {
            Program::Ffmpeg => "ffmpeg",
            Program::Ffprobe => "ffprobe",
            Program::Soffice => "soffice",
            Program::Pdftoppm => "pdftoppm",
            Program::Pdftotext => "pdftotext",
        }
    }

    /// Name shown to the user when the program is missing.
    pub fn display_name(self) -> &'static str {
        match self {
            Program::Ffmpeg | Program::Ffprobe => "FFmpeg",
            Program::Soffice => "LibreOffice",
            Program::Pdftoppm | Program::Pdftotext => "Poppler",
        }
    }
}

fn exe_file(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn in_path(name: &str) -> Option<PathBuf> {
    let file = exe_file(name);
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(&file))
            .find(|p| p.is_file())
    })
}

/// Sub-folders of `dir` whose name starts with `prefix` (e.g. versioned WinGet package folders).
fn dirs_starting_with(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_lowercase().starts_with(&prefix.to_lowercase()))
        })
        .collect();
    out.sort();
    out.reverse(); // newest version first
    out
}

/// Install locations that are usually not on PATH.
fn standard_locations(program: Program) -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let file = exe_file(program.exe_name());
    let mut dirs: Vec<PathBuf> = Vec::new();

    match program {
        Program::Ffmpeg | Program::Ffprobe => {
            if let Some(local) = env("LOCALAPPDATA") {
                dirs.push(local.join("Microsoft").join("WinGet").join("Links"));
                let pkgs = local.join("Microsoft").join("WinGet").join("Packages");
                for pkg in dirs_starting_with(&pkgs, "Gyan.FFmpeg")
                    .into_iter()
                    .chain(dirs_starting_with(&pkgs, "BtbN.FFmpeg"))
                {
                    for build in dirs_starting_with(&pkg, "ffmpeg") {
                        dirs.push(build.join("bin"));
                    }
                }
            }
            for key in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Some(pf) = env(key) {
                    dirs.push(pf.join("ffmpeg").join("bin"));
                }
            }
            dirs.push(PathBuf::from(r"C:\ffmpeg\bin"));
            dirs.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin"));
            if let Some(home) = env("USERPROFILE") {
                dirs.push(home.join("scoop").join("shims"));
            }
        }
        Program::Soffice => {
            for key in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
                if let Some(pf) = env(key) {
                    dirs.push(pf.join("LibreOffice").join("program"));
                }
            }
            if let Some(home) = env("USERPROFILE") {
                dirs.push(home.join("scoop").join("apps").join("libreoffice").join("current").join("LibreOffice").join("program"));
            }
            dirs.push(PathBuf::from("/Applications/LibreOffice.app/Contents/MacOS"));
        }
        Program::Pdftoppm | Program::Pdftotext => {
            if let Some(home) = env("USERPROFILE") {
                dirs.push(home.join("scoop").join("shims"));
            }
            dirs.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin"));
            if let Some(local) = env("LOCALAPPDATA") {
                let pkgs = local.join("Microsoft").join("WinGet").join("Packages");
                for pkg in dirs_starting_with(&pkgs, "oschwartz10612.Poppler") {
                    for build in dirs_starting_with(&pkg, "poppler") {
                        dirs.push(build.join("Library").join("bin"));
                    }
                }
            }
        }
    }

    dirs.into_iter().map(|d| d.join(&file)).collect()
}

/// Full path of a helper program, from PATH or a standard install location.
pub fn find(program: Program) -> Option<PathBuf> {
    in_path(program.exe_name())
        .or_else(|| standard_locations(program).into_iter().find(|p| p.is_file()))
        .or_else(|| {
            // ffprobe ships next to ffmpeg
            if program != Program::Ffprobe {
                return None;
            }
            let ffmpeg = find(Program::Ffmpeg)?;
            let probe = ffmpeg.with_file_name(exe_file("ffprobe"));
            probe.is_file().then_some(probe)
        })
}

/// A command for `program`, or a user-facing error if it is not installed.
pub fn command(program: Program) -> Result<Command, String> {
    let path = find(program).ok_or_else(|| format!("{} is not installed", program.display_name()))?;
    Ok(hidden(Command::new(path)))
}

/// Configure a command to run without a console window and without inheriting stdin.
pub fn hidden(mut cmd: Command) -> Command {
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Run a command to completion. On failure the error holds the last line the program printed.
pub fn run(cmd: Command, what: &str) -> Result<std::process::Output, String> {
    run_with_timeout(cmd, what, None)
}

/// Like [`run`], but kills the program after `timeout`.
pub fn run_with_timeout(mut cmd: Command, what: &str, timeout: Option<Duration>) -> Result<std::process::Output, String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("{what} could not start: {e}"))?;

    // Drain both pipes on threads so a chatty program never blocks on a full pipe.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_t = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = std::io::Read::read_to_end(p, &mut buf);
        }
        buf
    });
    let err_t = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = std::io::Read::read_to_end(p, &mut buf);
        }
        buf
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if timeout.is_some_and(|t| start.elapsed() > t) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("{what} took too long and was stopped"));
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            Err(e) => return Err(format!("{what} failed: {e}")),
        }
    };

    let output = std::process::Output {
        status,
        stdout: out_t.join().unwrap_or_default(),
        stderr: err_t.join().unwrap_or_default(),
    };
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!("{what} failed: {}", last_line(&output.stderr).unwrap_or_else(|| format!("exit code {:?}", output.status.code()))))
    }
}

fn last_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .last()
        .map(|l| l.chars().take(160).collect())
}

/// A private scratch folder that is deleted when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Result<Self, String> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("shiftwheel-{tag}-{}-{nanos}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create a temporary folder: {e}"))?;
        Ok(Self(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `file:///` URL for a local folder, as LibreOffice expects for `-env:UserInstallation`.
pub fn file_url(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'.' | b'-' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_encodes_spaces_and_drive_letters() {
        assert_eq!(file_url(Path::new(r"C:\Users\Jo Doe\x")), "file:///C:/Users/Jo%20Doe/x");
        assert_eq!(file_url(Path::new("/tmp/a b")), "file:///tmp/a%20b");
    }

    #[test]
    fn temp_dir_is_removed_on_drop() {
        let p = {
            let t = TempDir::new("test").unwrap();
            std::fs::write(t.path().join("f"), b"x").unwrap();
            t.path().to_path_buf()
        };
        assert!(!p.exists());
    }

    #[test]
    fn run_reports_last_stderr_line() {
        let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        if cfg!(windows) {
            cmd.args(["/C", "echo first 1>&2 && echo boom 1>&2 && exit 3"]);
        } else {
            cmd.args(["-c", "echo first >&2; echo boom >&2; exit 3"]);
        }
        let err = run(hidden(cmd), "Test").unwrap_err();
        assert!(err.contains("boom"), "{err}");
    }
}
