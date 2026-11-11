//! Optional GPU telemetry: NVML loaded from the installed Windows driver, or
//! `nvidia-smi` and the amdgpu sysfs files on Linux. No bundled driver DLLs and
//! no mandatory NVML runtime.
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct GpuMetrics {
    pub supported: bool,
    pub name: Option<String>,
    pub usage: Option<u32>,
    pub temp: Option<u32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub memory_pct: Option<f32>,
    pub clock_mhz: Option<u32>,
    pub mem_clock_mhz: Option<u32>,
    pub pstate: Option<u32>,
    pub power_watts: Option<f32>,
    pub power_limit_watts: Option<f32>,
    pub fan_pct: Option<u32>,
    pub fan_rpm: Option<u32>,
    pub status: Option<String>,
}

#[cfg(any(windows, test, target_os = "linux"))]
fn memory_pct(used: u64, total: u64) -> Option<f32> {
    (total > 0).then(|| ((used as f64 / total as f64 * 100.0).clamp(0.0, 100.0)) as f32)
}

enum Backend {
    #[cfg(windows)]
    Nvml(windows::Driver),
    #[cfg(target_os = "linux")]
    Linux(linux::Driver),
}

pub struct Gpu {
    backend: Option<Backend>,
    reason: String,
}

impl Gpu {
    pub fn new() -> Self {
        if std::env::var("LIQUID_DECK_GPU").is_ok_and(|value| value.eq_ignore_ascii_case("off")) {
            return Self {
                backend: None,
                reason: "GPU izleme kapalı".into(),
            };
        }
        platform::load()
    }

    pub fn sample(&self, active: bool) -> GpuMetrics {
        match &self.backend {
            #[cfg(windows)]
            Some(Backend::Nvml(driver)) => driver.sample(active),
            #[cfg(target_os = "linux")]
            Some(Backend::Linux(driver)) => driver.sample(active),
            _ => {
                let _ = active;
                GpuMetrics {
                    status: Some(self.reason.clone()),
                    ..Default::default()
                }
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::{windows, Backend, Gpu};

    pub fn load() -> Gpu {
        match windows::Driver::load() {
            Ok(driver) => Gpu {
                backend: Some(Backend::Nvml(driver)),
                reason: String::new(),
            },
            Err(reason) => Gpu {
                backend: None,
                reason,
            },
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{linux, Backend, Gpu};

    pub fn load() -> Gpu {
        match linux::Driver::detect() {
            Some(driver) => Gpu {
                backend: Some(Backend::Linux(driver)),
                reason: String::new(),
            },
            None => Gpu {
                backend: None,
                reason: "GPU bulunamadı (nvidia-smi yok, amdgpu sysfs okunamıyor)".into(),
            },
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::Gpu;

    pub fn load() -> Gpu {
        Gpu {
            backend: None,
            reason: "GPU izleme bu platformda desteklenmiyor".into(),
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::{memory_pct, GpuMetrics};
    use std::{
        ffi::{c_char, c_void, CStr},
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{FreeLibrary, HMODULE},
        System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
    };

    type Device = *mut c_void;
    type Init = unsafe extern "C" fn() -> i32;
    type GetHandle = unsafe extern "C" fn(u32, *mut Device) -> i32;
    type GetName = unsafe extern "C" fn(Device, *mut c_char, u32) -> i32;
    type GetUtil = unsafe extern "C" fn(Device, *mut Util) -> i32;
    type GetMemory = unsafe extern "C" fn(Device, *mut Memory) -> i32;
    type GetValue = unsafe extern "C" fn(Device, *mut u32) -> i32;
    type GetSelected = unsafe extern "C" fn(Device, u32, *mut u32) -> i32;

    #[repr(C)]
    #[derive(Default)]
    struct Util {
        gpu: u32,
        memory: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Memory {
        total: u64,
        free: u64,
        used: u64,
    }

    // Handles are stored as integers so the exclusively-owned sampler may move
    // to the Tokio server thread. NVML supports calls/shutdown from any thread.
    struct Library(usize);
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0 as HMODULE);
            }
        }
    }

    pub struct Driver {
        _library: Library,
        device: usize,
        shutdown: Init,
        name: String,
        util: Option<GetUtil>,
        memory: Option<GetMemory>,
        temperature: Option<GetSelected>,
        clock: Option<GetSelected>,
        power: Option<GetValue>,
        power_limit: Option<GetValue>,
        fan: Option<GetValue>,
        fan_rpm: Option<GetFanRpm>,
        pstate: Option<GetValue>,
    }

    /// nvmlFanSpeedInfo_v1: {version, fan, speed}. NVML versions structs as
    /// `sizeof | (version << 16)`.
    #[repr(C)]
    struct FanSpeedInfo {
        version: u32,
        fan: u32,
        speed: u32,
    }
    type GetFanRpm = unsafe extern "C" fn(Device, *mut FanSpeedInfo) -> i32;
    const FAN_SPEED_INFO_VERSION: u32 = (std::mem::size_of::<FanSpeedInfo>() as u32) | (1 << 16);

    // Each caller fixes the exact ABI signature defined by NVIDIA's NVML API.
    macro_rules! symbol {
        ($library:expr, $name:literal, $ty:ty) => {
            GetProcAddress($library.0 as HMODULE, concat!($name, "\0").as_ptr()).map(|address| {
                std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(address)
            })
        };
    }

    impl Driver {
        pub fn load() -> Result<Self, String> {
            unsafe {
                // Never search the working directory or download driver libraries.
                let name: Vec<u16> = "nvml.dll\0".encode_utf16().collect();
                let module =
                    LoadLibraryExW(name.as_ptr(), ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
                if module.is_null() {
                    return Err("NVIDIA NVML bulunamadı; sürücüyü kontrol edin".into());
                }
                let library = Library(module as usize);
                let init =
                    symbol!(library, "nvmlInit_v2", Init).ok_or("NVML başlatma API'si yok")?;
                let shutdown =
                    symbol!(library, "nvmlShutdown", Init).ok_or("NVML kapatma API'si yok")?;
                let get_handle = symbol!(library, "nvmlDeviceGetHandleByIndex_v2", GetHandle)
                    .ok_or("NVML aygıt API'si yok")?;
                let code = init();
                if code != 0 {
                    return Err(format!("NVML başlatılamadı (kod {code})"));
                }
                let mut device = ptr::null_mut();
                let code = get_handle(0, &mut device);
                if code != 0 || device.is_null() {
                    shutdown();
                    return Err(format!("NVIDIA GPU okunamıyor (kod {code})"));
                }
                let mut name_buffer = [0 as c_char; 128];
                let mut name = "NVIDIA GPU".to_string();
                if let Some(get_name) = symbol!(library, "nvmlDeviceGetName", GetName) {
                    if get_name(device, name_buffer.as_mut_ptr(), name_buffer.len() as u32) == 0 {
                        name_buffer[127] = 0;
                        name = CStr::from_ptr(name_buffer.as_ptr())
                            .to_string_lossy()
                            .into_owned();
                    }
                }
                Ok(Self {
                    device: device as usize,
                    shutdown,
                    name,
                    util: symbol!(library, "nvmlDeviceGetUtilizationRates", GetUtil),
                    memory: symbol!(library, "nvmlDeviceGetMemoryInfo", GetMemory),
                    temperature: symbol!(library, "nvmlDeviceGetTemperature", GetSelected),
                    clock: symbol!(library, "nvmlDeviceGetClockInfo", GetSelected),
                    power: symbol!(library, "nvmlDeviceGetPowerUsage", GetValue),
                    power_limit: symbol!(library, "nvmlDeviceGetEnforcedPowerLimit", GetValue),
                    fan: symbol!(library, "nvmlDeviceGetFanSpeed", GetValue),
                    fan_rpm: symbol!(library, "nvmlDeviceGetFanSpeedRPM", GetFanRpm),
                    pstate: symbol!(library, "nvmlDeviceGetPerformanceState", GetValue),
                    _library: library,
                })
            }
        }

        fn value(&self, function: Option<GetValue>) -> Option<u32> {
            let mut value = 0;
            function.and_then(|read| {
                (unsafe { read(self.device as Device, &mut value) } == 0).then_some(value)
            })
        }

        fn selected(&self, function: Option<GetSelected>) -> Option<u32> {
            self.selected_with(function, 0)
        }

        fn selected_with(&self, function: Option<GetSelected>, selector: u32) -> Option<u32> {
            let mut value = 0;
            function.and_then(|read| {
                (unsafe { read(self.device as Device, selector, &mut value) } == 0).then_some(value)
            })
        }

        pub fn sample(&self, active: bool) -> GpuMetrics {
            let mut data = GpuMetrics {
                supported: true,
                name: Some(self.name.clone()),
                ..Default::default()
            };
            if !active {
                data.status = Some("Panel beklemede".into());
                return data;
            }
            unsafe {
                let mut utilization = Util::default();
                if self
                    .util
                    .is_some_and(|read| read(self.device as Device, &mut utilization) == 0)
                {
                    data.usage = Some(utilization.gpu.min(100));
                }
                let mut memory = Memory::default();
                if self
                    .memory
                    .is_some_and(|read| read(self.device as Device, &mut memory) == 0)
                    && memory.total > 0
                {
                    data.memory_used = Some(memory.used);
                    data.memory_total = Some(memory.total);
                    data.memory_pct = memory_pct(memory.used, memory.total);
                }
            }
            data.temp = self.selected(self.temperature).filter(|value| *value < 200);
            data.clock_mhz = self.selected(self.clock);
            // NVML clock selector 2 is the memory clock.
            data.mem_clock_mhz = self.selected_with(self.clock, 2);
            data.pstate = self.value(self.pstate).filter(|value| *value <= 32);
            data.power_watts = self.value(self.power).map(|value| value as f32 / 1000.0);
            data.power_limit_watts = self
                .value(self.power_limit)
                .map(|value| value as f32 / 1000.0);
            data.fan_pct = self.value(self.fan).filter(|value| *value <= 100);
            data.fan_rpm = self.fan_rpm.and_then(|read| {
                let mut info = FanSpeedInfo {
                    version: FAN_SPEED_INFO_VERSION,
                    fan: 0,
                    speed: 0,
                };
                (unsafe { read(self.device as Device, &mut info) } == 0).then_some(info.speed)
            });
            if data.usage.is_none() && data.temp.is_none() && data.memory_used.is_none() {
                data.status = Some("GPU ölçümleri şu anda okunamıyor".into());
            }
            data
        }
    }

    impl Drop for Driver {
        fn drop(&mut self) {
            // Shutdown before Library's Drop unloads the function pointers.
            unsafe {
                (self.shutdown)();
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{memory_pct, GpuMetrics};
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
    };

    const QUERY: &str = "name,utilization.gpu,temperature.gpu,memory.used,memory.total,\
clocks.current.graphics,clocks.current.memory,power.draw,power.limit,fan.speed,pstate";
    const UNAVAILABLE: &str = "GPU ölçümleri şu anda okunamıyor";

    pub enum Driver {
        Nvidia { name: String },
        Amd { card: PathBuf, name: String },
    }

    impl Driver {
        pub fn detect() -> Option<Self> {
            if let Some(out) = nvidia(QUERY) {
                let name = out.split(',').next().unwrap_or("").trim();
                if !name.is_empty() {
                    return Some(Self::Nvidia {
                        name: name.to_string(),
                    });
                }
            }
            amd_card().map(|card| Self::Amd {
                card,
                name: "AMD Radeon".to_string(),
            })
        }

        pub fn sample(&self, active: bool) -> GpuMetrics {
            match self {
                Self::Nvidia { name } => sample_nvidia(name, active),
                Self::Amd { card, name } => sample_amd(card, name, active),
            }
        }
    }

    fn nvidia(query: &str) -> Option<String> {
        let query = format!("--query-gpu={query}");
        let output = Command::new("nvidia-smi")
            .args([query.as_str(), "--format=csv,noheader,nounits"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let line = text.lines().next()?.trim();
        (!line.is_empty() && !line.to_ascii_lowercase().contains("no devices"))
            .then(|| line.to_string())
    }

    fn number<T: std::str::FromStr>(raw: &str) -> Option<T> {
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('[') || raw.eq_ignore_ascii_case("n/a") {
            return None;
        }
        raw.parse().ok()
    }

    fn parse_pstate(raw: &str) -> Option<u32> {
        raw.trim()
            .trim_start_matches(['P', 'p'])
            .parse::<u32>()
            .ok()
            .filter(|value| *value <= 32)
    }

    fn sample_nvidia(name: &str, active: bool) -> GpuMetrics {
        let mut data = GpuMetrics {
            supported: true,
            name: Some(name.to_string()),
            ..Default::default()
        };
        if !active {
            data.status = Some("Panel beklemede".into());
            return data;
        }
        let Some(line) = nvidia(QUERY) else {
            data.status = Some(UNAVAILABLE.into());
            return data;
        };
        let parts: Vec<&str> = line.split(',').collect();
        let field = |index: usize| parts.get(index).copied().unwrap_or("");

        data.usage = number::<u32>(field(1)).map(|value| value.min(100));
        data.temp = number::<u32>(field(2)).filter(|value| *value < 200);

        let used = number::<f64>(field(3)).map(|mib| (mib * 1_048_576.0) as u64);
        let total = number::<f64>(field(4)).map(|mib| (mib * 1_048_576.0) as u64);
        if let (Some(used), Some(total)) = (used, total) {
            if total > 0 {
                data.memory_used = Some(used);
                data.memory_total = Some(total);
                data.memory_pct = memory_pct(used, total);
            }
        }

        data.clock_mhz = number::<u32>(field(5));
        data.mem_clock_mhz = number::<u32>(field(6));
        data.power_watts = number::<f32>(field(7));
        data.power_limit_watts = number::<f32>(field(8));
        data.fan_pct = number::<u32>(field(9)).filter(|value| *value <= 100);
        data.pstate = parse_pstate(field(10));

        if data.usage.is_none() && data.temp.is_none() && data.memory_used.is_none() {
            data.status = Some(UNAVAILABLE.into());
        }
        data
    }

    fn amd_card() -> Option<PathBuf> {
        let mut cards: Vec<PathBuf> = fs::read_dir("/sys/class/drm")
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                name.starts_with("card") && !name.contains('-')
            })
            .collect();
        cards.sort();
        cards
            .into_iter()
            .find(|card| card.join("device/gpu_busy_percent").exists())
    }

    fn read_number<T: std::str::FromStr>(path: &Path) -> Option<T> {
        fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    fn amd_hwmon(card: &Path) -> Option<PathBuf> {
        fs::read_dir(card.join("device/hwmon"))
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.join("temp1_input").exists())
    }

    fn sample_amd(card: &Path, name: &str, active: bool) -> GpuMetrics {
        let device = card.join("device");
        let mut data = GpuMetrics {
            supported: true,
            name: Some(name.to_string()),
            ..Default::default()
        };
        if !active {
            data.status = Some("Panel beklemede".into());
            return data;
        }
        let hwmon = amd_hwmon(card);
        let hwmon_file = |file: &str| hwmon.as_ref().map(|dir| dir.join(file));

        data.usage =
            read_number::<u32>(&device.join("gpu_busy_percent")).map(|value| value.min(100));
        data.temp = hwmon_file("temp1_input")
            .and_then(|path| read_number::<u32>(&path))
            .map(|milli| milli / 1000)
            .filter(|value| *value < 200);

        let used = read_number::<u64>(&device.join("mem_info_vram_used"));
        let total = read_number::<u64>(&device.join("mem_info_vram_total"));
        if let (Some(used), Some(total)) = (used, total) {
            if total > 0 {
                data.memory_used = Some(used);
                data.memory_total = Some(total);
                data.memory_pct = memory_pct(used, total);
            }
        }

        data.power_watts = hwmon_file("power1_average")
            .and_then(|path| read_number::<u64>(&path))
            .map(|micro| micro as f32 / 1_000_000.0);
        data.fan_rpm = hwmon_file("fan1_input").and_then(|path| read_number::<u32>(&path));
        data.fan_pct = hwmon_file("pwm1")
            .and_then(|path| read_number::<u32>(&path))
            .map(|pwm| ((pwm.min(255) as f32 / 255.0) * 100.0).round() as u32);

        if data.usage.is_none() && data.temp.is_none() {
            data.status = Some(UNAVAILABLE.into());
        }
        data
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn unavailable_values_become_none() {
            assert_eq!(number::<u32>("[N/A]"), None);
            assert_eq!(number::<u32>("N/A"), None);
            assert_eq!(number::<u32>("  "), None);
            assert_eq!(number::<u32>("41"), Some(41));
            assert_eq!(number::<f32>("13.83"), Some(13.83));
        }

        #[test]
        fn performance_states_parse_within_range() {
            assert_eq!(parse_pstate("P5"), Some(5));
            assert_eq!(parse_pstate("P0"), Some(0));
            assert_eq!(parse_pstate("P12"), Some(12));
            assert_eq!(parse_pstate("N/A"), None);
            assert_eq!(parse_pstate("P40"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_gpu_values_are_null_not_zero() {
        let json = serde_json::to_value(GpuMetrics::default()).unwrap();
        assert_eq!(json["supported"], false);
        for field in [
            "usage",
            "temp",
            "memory_used",
            "memory_pct",
            "clock_mhz",
            "power_watts",
            "fan_pct",
        ] {
            assert!(json[field].is_null(), "{field} must remain missing");
        }
    }
    #[test]
    fn memory_percentage_handles_invalid_totals() {
        assert_eq!(memory_pct(1, 0), None);
        assert_eq!(memory_pct(2, 8), Some(25.0));
        assert_eq!(memory_pct(9, 8), Some(100.0));
    }
}
