use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use windows_sys::Win32::Foundation::{HGLOBAL, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND};
use windows_sys::Win32::UI::Shell::DROPFILES;

const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;

pub fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    unsafe {
        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes_len = utf16.len() * std::mem::size_of::<u16>();

        let h_mem: HGLOBAL = GlobalAlloc(GHND, bytes_len);
        if h_mem == 0 as HGLOBAL {
            return Err("Failed to allocate global memory".into());
        }

        let ptr = GlobalLock(h_mem) as *mut u16;
        if ptr.is_null() {
            return Err("Failed to lock global memory".into());
        }
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
        GlobalUnlock(h_mem);

        if OpenClipboard(0 as HWND) == 0 {
            return Err("Failed to open clipboard".into());
        }
        EmptyClipboard();
        SetClipboardData(CF_UNICODETEXT, h_mem as _);
        CloseClipboard();
        Ok(())
    }
}

pub fn copy_files_to_clipboard(paths: &[String]) -> Result<(), String> {
    unsafe {
        let mut wide_bytes: Vec<u8> = Vec::new();
        for p in paths {
            let wide: Vec<u16> = p.encode_utf16().chain(std::iter::once(0)).collect();
            let slice = std::slice::from_raw_parts(
                wide.as_ptr() as *const u8,
                wide.len() * std::mem::size_of::<u16>(),
            );
            wide_bytes.extend_from_slice(slice);
        }
        // Double-null terminator (two 0 u16 = 4 bytes of 0)
        wide_bytes.extend_from_slice(&[0, 0]);

        let dropfiles_size = std::mem::size_of::<DROPFILES>();
        let total_size = dropfiles_size + wide_bytes.len();

        let h_mem: HGLOBAL = GlobalAlloc(GHND, total_size);
        if h_mem == 0 as HGLOBAL {
            return Err("Failed to allocate global memory for files".into());
        }

        let ptr = GlobalLock(h_mem) as *mut u8;
        if ptr.is_null() {
            return Err("Failed to lock global memory".into());
        }

        let df = DROPFILES {
            pFiles: dropfiles_size as u32,
            pt: std::mem::zeroed(),
            fNC: 0,
            fWide: 1, // Unicode
        };
        std::ptr::copy_nonoverlapping(
            &df as *const _ as *const u8,
            ptr,
            dropfiles_size,
        );
        std::ptr::copy_nonoverlapping(
            wide_bytes.as_ptr(),
            ptr.add(dropfiles_size),
            wide_bytes.len(),
        );

        GlobalUnlock(h_mem);

        if OpenClipboard(0 as HWND) == 0 {
            return Err("Failed to open clipboard".into());
        }
        EmptyClipboard();
        SetClipboardData(CF_HDROP, h_mem as _);
        CloseClipboard();
        Ok(())
    }
}

pub fn open_terminal(paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let p = Path::new(&paths[0]);
    let dir = if p.is_dir() {
        p.to_path_buf()
    } else {
        p.parent().unwrap_or(Path::new(".")).to_path_buf()
    };

    if Command::new("wt.exe")
        .args(["-d", &dir.to_string_lossy()])
        .spawn()
        .is_err()
    {
        let _ = Command::new("cmd.exe")
            .arg("/K")
            .current_dir(&dir)
            .spawn();
    }
}

pub fn reveal_in_explorer(path: &str) {
    let p = Path::new(path);
    if p.exists() {
        let _ = Command::new("explorer.exe")
            .arg(format!("/select,{}", path))
            .spawn();
    }
}

pub fn pick_folder() -> Option<String> {
    // Quick PowerShell folder dialog with windowsHide
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = "Select target folder"
$dialog.ShowNewFolderButton = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    Write-Output $dialog.SelectedPath
}
"#;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

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

pub fn transfer_files(paths: &[String], is_move: bool) -> Result<String, String> {
    let target_dir_str = match pick_folder() {
        Some(d) => d,
        None => return Ok("Cancelled".into()),
    };
    let target_dir = Path::new(&target_dir_str);
    let mut count = 0;

    for src_str in paths {
        let src = Path::new(src_str);
        if !src.exists() {
            continue;
        }
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("");
        let dest = unique_path(target_dir, stem, ext);

        if is_move {
            if fs::rename(src, &dest).is_err() {
                // cross-device move fallback: copy then remove
                fs::copy(src, &dest).map_err(|e| e.to_string())?;
                if src.is_dir() {
                    let _ = fs::remove_dir_all(src);
                } else {
                    let _ = fs::remove_file(src);
                }
            }
        } else {
            if src.is_dir() {
                let _ = fs::copy(src, &dest);
            } else {
                fs::copy(src, &dest).map_err(|e| e.to_string())?;
            }
        }
        count += 1;
    }

    Ok(format!(
        "{} items {} to {}",
        count,
        if is_move { "moved" } else { "copied" },
        target_dir_str
    ))
}
