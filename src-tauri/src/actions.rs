//! File actions: clipboard, copy/move to a folder, terminal, reveal in Explorer.

use std::fs;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command;

/// `dir/stem.ext`, or `dir/stem (1).ext`, `dir/stem (2).ext`, ... if that name is taken.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let make_name = |s: &str| -> String {
        if ext.is_empty() {
            s.to_string()
        } else {
            format!("{s}.{ext}")
        }
    };
    let mut candidate = dir.join(make_name(stem));
    let mut i = 1;
    while candidate.exists() {
        candidate = dir.join(make_name(&format!("{stem} ({i})")));
        i += 1;
    }
    candidate
}

/// Free destination for `src` inside `target_dir`, keeping folder names whole.
fn destination(src: &Path, target_dir: &Path) -> PathBuf {
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "item".into());
    if src.is_dir() {
        unique_path(target_dir, &name, "")
    } else {
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(name);
        let ext = src.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
        unique_path(target_dir, &stem, &ext)
    }
}

/// Copy a file or a whole folder tree.
pub fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dest).map(|_| ())
    }
}

/// Result of copying/moving a selection.
#[derive(Debug, Default)]
pub struct Transfer {
    pub done: Vec<PathBuf>,
    pub errors: Vec<String>,
}

/// Copy or move every path into `target_dir`. Name clashes get `(1)`, `(2)`, ...
pub fn transfer_files(paths: &[String], target_dir: &Path, is_move: bool) -> Transfer {
    let mut result = Transfer::default();
    let target_canon = fs::canonicalize(target_dir).unwrap_or_else(|_| target_dir.to_path_buf());

    for src_str in paths {
        let src = Path::new(src_str);
        let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| src_str.clone());
        if !src.exists() {
            result.errors.push(format!("{name}: no longer exists"));
            continue;
        }
        if src.is_dir() {
            let src_canon = fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
            if target_canon.starts_with(&src_canon) {
                result.errors.push(format!("{name}: can't put a folder inside itself"));
                continue;
            }
        }
        if is_move && src.parent().and_then(|p| fs::canonicalize(p).ok()) == Some(target_canon.clone()) {
            result.errors.push(format!("{name}: already in that folder"));
            continue;
        }

        let dest = destination(src, target_dir);
        let outcome = if is_move {
            fs::rename(src, &dest).or_else(|_| {
                // Different drive: copy everything first, delete the original only if that worked.
                copy_recursive(src, &dest).and_then(|_| if src.is_dir() { fs::remove_dir_all(src) } else { fs::remove_file(src) })
            })
        } else {
            copy_recursive(src, &dest)
        };
        match outcome {
            Ok(()) => result.done.push(dest),
            Err(e) => {
                // Don't leave a half-copied item behind.
                if !is_move || src.exists() {
                    if dest.is_dir() {
                        let _ = fs::remove_dir_all(&dest);
                    } else {
                        let _ = fs::remove_file(&dest);
                    }
                }
                result.errors.push(format!("{name}: {e}"));
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------------------------
// Windows-only actions
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
mod clipboard {
    use windows_sys::Win32::Foundation::{GlobalFree, HGLOBAL, HWND};
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows_sys::Win32::System::Memory::{GHND, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows_sys::Win32::UI::Shell::DROPFILES;

    pub const CF_UNICODETEXT: u32 = 13;
    pub const CF_HDROP: u32 = 15;

    /// Put `bytes` on the clipboard as `format`. Retries while another app holds the clipboard.
    pub fn set(format: u32, bytes: &[u8]) -> Result<(), String> {
        unsafe {
            let h_mem: HGLOBAL = GlobalAlloc(GHND, bytes.len());
            if h_mem.is_null() {
                return Err("Out of memory".into());
            }
            let ptr = GlobalLock(h_mem) as *mut u8;
            if ptr.is_null() {
                GlobalFree(h_mem);
                return Err("Could not lock clipboard memory".into());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            GlobalUnlock(h_mem);

            let mut opened = false;
            for _ in 0..20 {
                if OpenClipboard(0 as HWND) != 0 {
                    opened = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            if !opened {
                GlobalFree(h_mem);
                return Err("The clipboard is busy".into());
            }
            EmptyClipboard();
            let ok = !SetClipboardData(format, h_mem as _).is_null();
            CloseClipboard();
            if !ok {
                // Ownership only passes to the system on success.
                GlobalFree(h_mem);
                return Err("Could not write to the clipboard".into());
            }
            Ok(())
        }
    }

    pub fn text_bytes(text: &str) -> Vec<u8> {
        text.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect()
    }

    /// DROPFILES header followed by a double-NUL-terminated UTF-16 path list.
    pub fn hdrop_bytes(paths: &[String]) -> Vec<u8> {
        let header = std::mem::size_of::<DROPFILES>();
        let mut bytes = vec![0u8; header];
        let df = DROPFILES { pFiles: header as u32, pt: unsafe { std::mem::zeroed() }, fNC: 0, fWide: 1 };
        unsafe { std::ptr::copy_nonoverlapping(&df as *const DROPFILES as *const u8, bytes.as_mut_ptr(), header) };
        for p in paths {
            bytes.extend(p.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes));
        }
        bytes.extend_from_slice(&[0, 0]);
        bytes
    }
}

#[cfg(windows)]
pub fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    clipboard::set(clipboard::CF_UNICODETEXT, &clipboard::text_bytes(text))
}

#[cfg(windows)]
pub fn copy_files_to_clipboard(paths: &[String]) -> Result<(), String> {
    clipboard::set(clipboard::CF_HDROP, &clipboard::hdrop_bytes(paths))
}

#[cfg(not(windows))]
pub fn copy_text_to_clipboard(_text: &str) -> Result<(), String> {
    Err("Clipboard actions need Windows".into())
}

#[cfg(not(windows))]
pub fn copy_files_to_clipboard(_paths: &[String]) -> Result<(), String> {
    Err("Clipboard actions need Windows".into())
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Folder to open a terminal in: the folder itself, or the folder containing the file.
pub fn terminal_dir(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
    }
}

#[cfg(windows)]
pub fn open_terminal(paths: &[String]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let first = paths.first().ok_or("Nothing selected")?;
    let dir = terminal_dir(Path::new(first));
    let wt = crate::external::hidden(Command::new("wt.exe")).arg("-d").arg(&dir).spawn();
    if wt.is_ok() {
        return Ok(());
    }
    Command::new("cmd.exe")
        .arg("/K")
        .current_dir(&dir)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open a terminal: {e}"))
}

#[cfg(not(windows))]
pub fn open_terminal(_paths: &[String]) -> Result<(), String> {
    Err("Opening a terminal needs Windows".into())
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Explorer's `/select,` switch takes the path quoted on its own, which Rust's default
/// argument quoting doesn't produce for paths with spaces.
pub fn explorer_select_arg(path: &str) -> String {
    format!("/select,\"{}\"", path.replace('"', ""))
}

#[cfg(windows)]
pub fn reveal_in_explorer(path: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    if !Path::new(path).exists() {
        return Err("The file no longer exists".into());
    }
    Command::new("explorer.exe")
        .raw_arg(explorer_select_arg(path))
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open Explorer: {e}"))
}

#[cfg(not(windows))]
pub fn reveal_in_explorer(path: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        Ok(())
    } else {
        Err("The file no longer exists".into())
    }
}

/// Ask for a target folder. None when the dialog is cancelled.
#[cfg(windows)]
pub fn pick_folder() -> Option<PathBuf> {
    // -STA is required by the WinForms dialog; the TopMost owner keeps it in front.
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$owner = New-Object System.Windows.Forms.Form -Property @{ TopMost = $true; ShowInTaskbar = $false }
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = "Select target folder"
$dialog.ShowNewFolderButton = $true
if ($dialog.ShowDialog($owner) -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Out.Write($dialog.SelectedPath)
}
$owner.Dispose()
"#;
    let mut cmd = crate::external::hidden(Command::new("powershell.exe"));
    cmd.args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script]);
    let output = crate::external::run(cmd, "Folder picker").ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(PathBuf::from(path)) }
}

#[cfg(not(windows))]
pub fn pick_folder() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::external::TempDir;

    #[test]
    fn unique_path_never_reuses_a_name() {
        let t = TempDir::new("unique").unwrap();
        let a = unique_path(t.path(), "a", "txt");
        fs::write(&a, "1").unwrap();
        let b = unique_path(t.path(), "a", "txt");
        assert_eq!(b.file_name().unwrap(), "a (1).txt");
    }

    #[test]
    fn copy_and_move_folders_and_files() {
        let t = TempDir::new("transfer").unwrap();
        let src_dir = t.path().join("src folder");
        fs::create_dir_all(src_dir.join("nested")).unwrap();
        fs::write(src_dir.join("nested").join("x.txt"), "x").unwrap();
        let file = t.path().join("note.v2.txt");
        fs::write(&file, "n").unwrap();
        let target = t.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("note.v2.txt"), "existing").unwrap();

        let paths = vec![src_dir.to_string_lossy().to_string(), file.to_string_lossy().to_string()];
        let r = transfer_files(&paths, &target, false);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(fs::read_to_string(target.join("src folder").join("nested").join("x.txt")).unwrap(), "x");
        assert_eq!(fs::read_to_string(target.join("note.v2 (1).txt")).unwrap(), "n");
        assert_eq!(fs::read_to_string(target.join("note.v2.txt")).unwrap(), "existing");
        assert!(src_dir.exists() && file.exists(), "copy must keep the originals");

        let r = transfer_files(&paths, &target, true);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(!src_dir.exists() && !file.exists());
        assert!(target.join("src folder (1)").join("nested").join("x.txt").exists());
    }

    #[test]
    fn refuses_folder_into_itself() {
        let t = TempDir::new("self").unwrap();
        let dir = t.path().join("d");
        fs::create_dir_all(dir.join("inner")).unwrap();
        let r = transfer_files(&[dir.to_string_lossy().to_string()], &dir.join("inner"), true);
        assert_eq!(r.done.len(), 0);
        assert_eq!(r.errors.len(), 1);
        assert!(dir.exists());
    }

    #[test]
    fn explorer_arg_quotes_path() {
        assert_eq!(explorer_select_arg(r"C:\a b\c.txt"), r#"/select,"C:\a b\c.txt""#);
    }

    #[cfg(windows)]
    #[test]
    fn clipboard_roundtrip() {
        use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
        use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
        use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};

        copy_text_to_clipboard("C:\\a b\\ä.txt\r\nsecond").unwrap();
        unsafe {
            assert!(OpenClipboard(std::ptr::null_mut()) != 0);
            let h = GetClipboardData(clipboard::CF_UNICODETEXT);
            assert!(!h.is_null());
            let p = GlobalLock(h as _) as *const u16;
            let len = (0..).take_while(|&i| *p.add(i) != 0).count();
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            GlobalUnlock(h as _);
            CloseClipboard();
            assert_eq!(text, "C:\\a b\\ä.txt\r\nsecond");
        }

        copy_files_to_clipboard(&["C:\\one.txt".into(), "C:\\two words.png".into()]).unwrap();
        unsafe {
            assert!(OpenClipboard(std::ptr::null_mut()) != 0);
            let h = GetClipboardData(clipboard::CF_HDROP) as HDROP;
            assert!(!h.is_null());
            let n = DragQueryFileW(h, u32::MAX, std::ptr::null_mut(), 0);
            let mut names = Vec::new();
            for i in 0..n {
                let mut buf = [0u16; 260];
                let l = DragQueryFileW(h, i, buf.as_mut_ptr(), 260) as usize;
                names.push(String::from_utf16_lossy(&buf[..l]));
            }
            CloseClipboard();
            assert_eq!(names, vec!["C:\\one.txt", "C:\\two words.png"]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn hdrop_layout_is_double_nul_terminated() {
        let b = clipboard::hdrop_bytes(&["C:\\a".into(), "C:\\b".into()]);
        let header = std::mem::size_of::<windows_sys::Win32::UI::Shell::DROPFILES>();
        assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize, header);
        assert_eq!(&b[b.len() - 4..], &[0, 0, 0, 0]);
    }
}
