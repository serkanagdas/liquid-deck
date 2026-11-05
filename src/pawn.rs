//! Shared loader for the already-installed PawnIO driver and its signed,
//! unmodified LibreHardwareMonitor modules. Never installs or changes drivers.
#[cfg(windows)]
pub struct PawnModule {
    library: usize,
    handle: usize,
    execute: unsafe extern "system" fn(
        windows_sys::Win32::Foundation::HANDLE,
        *const u8,
        *const u64,
        usize,
        *mut u64,
        usize,
        *mut usize,
    ) -> i32,
    close: unsafe extern "system" fn(windows_sys::Win32::Foundation::HANDLE) -> i32,
}

#[cfg(windows)]
impl PawnModule {
    /// Loads a signed module into the machine's existing PawnIO installation.
    pub fn load(module_bytes: &[u8], what: &str) -> Result<Self, String> {
        use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
        use windows_sys::Win32::{
            Foundation::{FreeLibrary, HANDLE},
            System::LibraryLoader::*,
        };
        type Open = unsafe extern "system" fn(*mut HANDLE) -> i32;
        type Load = unsafe extern "system" fn(HANDLE, *const u8, usize) -> i32;
        type Execute = unsafe extern "system" fn(
            HANDLE,
            *const u8,
            *const u64,
            usize,
            *mut u64,
            usize,
            *mut usize,
        ) -> i32;
        type Close = unsafe extern "system" fn(HANDLE) -> i32;

        unsafe {
            let base = std::env::var_os("ProgramW6432")
                .or_else(|| std::env::var_os("ProgramFiles"))
                .ok_or("Program Files bulunamadı")?;
            let path = std::path::Path::new(&base).join("PawnIO/PawnIOLib.dll");
            let name: Vec<u16> = OsStr::new(&path).encode_wide().chain(Some(0)).collect();
            let library = LoadLibraryExW(
                name.as_ptr(),
                ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            );
            if library.is_null() {
                return Err("Mevcut PawnIOLib.dll açılamadı".into());
            }

            macro_rules! symbol {
                ($name:literal, $ty:ty) => {
                    GetProcAddress(library, concat!($name, "\0").as_ptr()).map(|p| {
                        std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(p)
                    })
                };
            }

            let (Some(open), Some(load), Some(execute), Some(close)) = (
                symbol!("pawnio_open", Open),
                symbol!("pawnio_load", Load),
                symbol!("pawnio_execute", Execute),
                symbol!("pawnio_close", Close),
            ) else {
                FreeLibrary(library);
                return Err("PawnIO API bulunamadı".into());
            };

            let mut handle = ptr::null_mut();
            if open(&mut handle) < 0 {
                FreeLibrary(library);
                return Err("PawnIO erişimi için yardımcıyı yönetici olarak çalıştırın".into());
            }
            // PawnIO verifies the module signature; the module stays unmodified.
            if load(handle, module_bytes.as_ptr(), module_bytes.len()) < 0 {
                close(handle);
                FreeLibrary(library);
                return Err(format!("İmzalı {what} modülü yüklenemedi"));
            }
            Ok(Self {
                library: library as usize,
                handle: handle as usize,
                execute,
                close,
            })
        }
    }

    /// Executes one module function; input/output are 64-bit slots.
    pub fn execute(&self, name: &str, input: &[u64], output: &mut [u64]) -> bool {
        let mut function = [0u8; 32];
        let len = name.len().min(31);
        function[..len].copy_from_slice(&name.as_bytes()[..len]);
        let mut written = 0usize;
        unsafe {
            (self.execute)(
                self.handle as windows_sys::Win32::Foundation::HANDLE,
                function.as_ptr(),
                input.as_ptr(),
                input.len(),
                output.as_mut_ptr(),
                output.len(),
                &mut written,
            ) >= 0
        }
    }
}

#[cfg(windows)]
impl Drop for PawnModule {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::{FreeLibrary, HANDLE, HMODULE};
        // Close the executor before unloading PawnIOLib so its function
        // pointers stay valid while closing.
        unsafe {
            (self.close)(self.handle as HANDLE);
            FreeLibrary(self.library as HMODULE);
        }
    }
}
