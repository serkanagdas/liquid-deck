//! Per-user logon startup; no administrator rights, service or background timer.
use std::{env, ffi::OsString, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::*,
    System::{Registry::*, Threading::*},
};

const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APPROVED: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";
const NAME: &str = "Liquid Deck";
pub const LEASE_NAME: &str = "Local\\LiquidDeck.Session.v1";
pub const SENSOR_TASK: &str = "Liquid Deck CPU Sensor";

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

/// MSVC/CommandLineToArgvW quoting, including embedded quotes and trailing slashes.
pub fn quote(value: &str) -> String {
    let mut result = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        if ch == '"' {
            result.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            result.extend(std::iter::repeat_n('\\', slashes));
        }
        slashes = 0;
        result.push(ch);
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    result
}

fn command() -> Result<String, String> {
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let mut parts = vec![quote(&exe.to_string_lossy())];
    for arg in env::args().skip(1).filter(|arg| {
        !matches!(
            arg.as_str(),
            "--enable-startup" | "--disable-startup" | "--console"
        )
    }) {
        parts.push(quote(&arg));
    }
    let command = parts.join(" ");
    if command.encode_utf16().count() > 260 {
        return Err(
            "Otomatik başlangıç komutu 260 karakteri aşıyor; EXE yolunu veya seçenekleri kısaltın."
                .into(),
        );
    }
    Ok(command)
}

fn read_value(key: &str, kind: u32) -> Option<Vec<u8>> {
    unsafe {
        let mut length = 0;
        if RegGetValueW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            wide(NAME).as_ptr(),
            kind,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut length,
        ) != 0
        {
            return None;
        }
        let mut data = vec![0u8; length as usize];
        (RegGetValueW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            wide(NAME).as_ptr(),
            kind,
            ptr::null_mut(),
            data.as_mut_ptr().cast(),
            &mut length,
        ) == 0)
            .then_some(data)
    }
}

pub fn enabled() -> bool {
    let Some(data) = read_value(RUN, RRF_RT_REG_SZ) else {
        return false;
    };
    let words: Vec<u16> = data
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|w| *w != 0)
        .collect();
    let value = String::from_utf16_lossy(&words);
    let Ok(exe) = env::current_exe() else {
        return false;
    };
    if !value.starts_with(&quote(&exe.to_string_lossy())) {
        return false;
    }
    !read_value(APPROVED, RRF_RT_REG_BINARY).is_some_and(|data| matches!(data.first(), Some(3 | 7)))
}

pub fn set_enabled(enable: bool) -> Result<(), String> {
    unsafe {
        let mut key = ptr::null_mut();
        let code = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN).as_ptr(),
            0,
            ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            ptr::null(),
            &mut key,
            ptr::null_mut(),
        );
        if code != 0 {
            return Err(format!("Başlangıç kaydı açılamadı ({code})"));
        }
        let result = if enable {
            match command() {
                Ok(command) => {
                    let data = wide(&command);
                    RegSetValueExW(
                        key,
                        wide(NAME).as_ptr(),
                        0,
                        REG_SZ,
                        data.as_ptr().cast(),
                        (data.len() * 2) as u32,
                    )
                }
                Err(error) => {
                    RegCloseKey(key);
                    return Err(error);
                }
            }
        } else {
            RegDeleteValueW(key, wide(NAME).as_ptr())
        };
        RegCloseKey(key);
        if result != 0 && !(result == ERROR_FILE_NOT_FOUND && !enable) {
            return Err(format!("Başlangıç kaydı değiştirilemedi ({result})"));
        }
        if enable {
            // Explicitly enabling startup also clears Task Manager's disabled
            // state for this application's own entry, never another entry.
            let mut approved = ptr::null_mut();
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(APPROVED).as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut approved,
            ) == 0
            {
                RegDeleteValueW(approved, wide(NAME).as_ptr());
                RegCloseKey(approved);
            }
        }
    }
    Ok(())
}

pub struct Instance(HANDLE);
impl Instance {
    pub fn acquire() -> Result<Option<Self>, String> {
        unsafe {
            let handle = CreateMutexW(ptr::null(), 1, wide(LEASE_NAME).as_ptr());
            if handle.is_null() {
                return Err("Liquid Deck oturum kilidi oluşturulamadı".into());
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let wait = WaitForSingleObject(handle, 200);
                if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
                    CloseHandle(handle);
                    return Ok(None);
                }
            }
            Ok(Some(Self(handle)))
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

pub fn adb_path() -> Result<OsString, String> {
    if let Some(path) = env::var_os("LIQUID_DECK_ADB") {
        if Path::new(&path).is_absolute() && Path::new(&path).is_file() {
            return Ok(path);
        }
        return Err("LIQUID_DECK_ADB geçerli bir mutlak adb.exe yolu olmalı".into());
    }
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path).map(|dir| dir.join("adb.exe")));
    }
    for name in ["ANDROID_SDK_ROOT", "ANDROID_HOME"] {
        if let Some(path) = env::var_os(name) {
            candidates.push(Path::new(&path).join("platform-tools/adb.exe"));
        }
    }
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        let local = Path::new(&local);
        candidates.push(local.join("Android/Sdk/platform-tools/adb.exe"));
        candidates.push(local.join("Microsoft/WinGet/Links/adb.exe"));
        if let Ok(entries) = std::fs::read_dir(local.join("Microsoft/WinGet/Packages")) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("Google.PlatformTools_")
                {
                    candidates.push(entry.path().join("platform-tools/adb.exe"));
                }
            }
        }
    }
    candidates
        .into_iter()
        .find(|path| path.is_absolute() && path.is_file())
        .map(|path| path.into_os_string())
        .ok_or_else(|| {
            "ADB bulunamadı. Platform-Tools kurun veya LIQUID_DECK_ADB ile yolunu belirtin.".into()
        })
}

pub async fn start_sensor_task() {
    let Some(windows) = env::var_os("SystemRoot") else {
        return;
    };
    let mut command =
        tokio::process::Command::new(Path::new(&windows).join("System32/schtasks.exe"));
    command
        .args(["/Run", "/TN", SENSOR_TASK])
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), command.output()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_windows_arguments() {
        assert_eq!(quote(""), "\"\"");
        assert_eq!(
            quote("C:\\Program Files\\deck.exe"),
            "\"C:\\Program Files\\deck.exe\""
        );
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("C:\\tail\\"), "\"C:\\tail\\\\\"");
    }
}
