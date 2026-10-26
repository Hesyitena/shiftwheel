use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
#[cfg(windows)]
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ,
};

#[cfg(windows)]
const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0";
#[cfg(windows)]
const APP_NAME: &str = "Shiftwheel\0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_true")]
    pub sound: bool,
    #[serde(default = "default_false")]
    pub first_run: bool,
    /// Tray "Enabled" toggle, kept across restarts.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            sound: true,
            first_run: false,
            enabled: true,
        }
    }
}

pub fn get_settings_path() -> PathBuf {
    let mut dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    dir.push("Shiftwheel");
    let _ = fs::create_dir_all(&dir);
    dir.push("settings.json");
    dir
}

pub fn load_settings() -> AppSettings {
    let path = get_settings_path();
    if let Ok(data) = fs::read_to_string(path) {
        if let Ok(settings) = serde_json::from_str(&data) {
            return settings;
        }
    }
    AppSettings::default()
}

pub fn save_settings(settings: &AppSettings) {
    let path = get_settings_path();
    if let Ok(data) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(path, data);
    }
}

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(not(windows))]
pub fn is_autostart_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set_autostart(_enabled: bool) {}

#[cfg(windows)]
pub fn is_autostart_enabled() -> bool {
    unsafe {
        let subkey = to_wide(RUN_SUBKEY);
        let val_name = to_wide(APP_NAME);
        let mut hkey: HKEY = std::mem::zeroed();

        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
            let mut val_type: u32 = 0;
            let mut data_len: u32 = 0;
            let status = RegQueryValueExW(
                hkey,
                val_name.as_ptr(),
                std::ptr::null_mut(),
                &mut val_type,
                std::ptr::null_mut(),
                &mut data_len,
            );
            RegCloseKey(hkey);
            status == 0
        } else {
            false
        }
    }
}

#[cfg(windows)]
pub fn set_autostart(enabled: bool) {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let cmd = format!("\"{}\" --hidden", exe_path.to_string_lossy());

    unsafe {
        let subkey = to_wide(RUN_SUBKEY);
        let val_name = to_wide(APP_NAME);
        let mut hkey: HKEY = std::mem::zeroed();

        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_WRITE, &mut hkey) == 0 {
            if enabled {
                let cmd_wide = to_wide(&cmd);
                let byte_len = (cmd_wide.len() * std::mem::size_of::<u16>()) as u32;
                RegSetValueExW(
                    hkey,
                    val_name.as_ptr(),
                    0,
                    REG_SZ,
                    cmd_wide.as_ptr() as *const u8,
                    byte_len,
                );
            } else {
                RegDeleteValueW(hkey, val_name.as_ptr());
            }
            RegCloseKey(hkey);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn autostart_roundtrip_restores_previous_state() {
        let before = is_autostart_enabled();
        set_autostart(true);
        assert!(is_autostart_enabled());
        set_autostart(false);
        assert!(!is_autostart_enabled());
        set_autostart(before);
    }
}
