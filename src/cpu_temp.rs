//! Read-only Ryzen sensor helper data path: CPU temperature and package
//! power through the existing PawnIO driver, plus motherboard fan speeds
//! through the signed LpcIO module. A 24/40-byte session-local mapping
//! shares only validated samples with the unelevated server process.
#[cfg(windows)]
use std::sync::atomic::Ordering;
#[cfg(windows)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Zen 4 THM_TCON_CUR_TMP: whole degrees in bits [31:21], 0.5 °C flag at 19.
#[cfg(any(windows, test))]
pub fn decode_zen4_temperature(raw: u32) -> Option<f32> {
    let temperature = (raw >> 21) as f32 * 0.125 - if raw & 0x80000 != 0 { 49.0 } else { 0.0 };
    (temperature > 0.0 && temperature < 125.0).then_some(temperature)
}

#[cfg(any(windows, test))]
fn validate_sample(
    before: u32,
    after: u32,
    status: u32,
    timestamp: u64,
    value: f32,
    now: u64,
) -> Option<f32> {
    (before == after
        && before & 1 == 0
        && status == 0
        && now.abs_diff(timestamp) <= 8000
        && value.is_finite()
        && value > 0.0
        && value < 125.0)
        .then_some(value)
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::superio::SuperIoProbe;
    use std::{
        ffi::c_void,
        ptr,
        sync::atomic::{AtomicU32, AtomicU64},
    };
    use windows_sys::Win32::{
        Foundation::*,
        System::{Memory::*, Threading::*},
    };

    pub const MAPPING: &str = "Local\\LiquidDeck.CpuTemperature.v2";
    const MAGIC: u32 = 0x4c444354;
    const PCI_MUTEX: &str = "Global\\Access_PCI";
    const FAN_SLOTS: usize = 8;
    const MSR_PWR_UNIT: u64 = 0xC001_0299;
    const MSR_PKG_ENERGY_STAT: u64 = 0xC001_029B;

    fn string(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    /// Session-local, sequence-guarded snapshot. u32::MAX marks an unknown
    /// or missing fan; power is NaN until the first valid delta exists.
    #[repr(C, align(8))]
    struct Shared {
        magic: AtomicU32,
        sequence: AtomicU32,
        timestamp: AtomicU64,
        temperature: AtomicU32,
        status: AtomicU32,
        power_watts: AtomicU32,
        fans: [AtomicU32; FAN_SLOTS],
        fan_pct: AtomicU32,
        fan_chip: [AtomicU32; 32],
    }

    struct Mapping {
        handle: usize,
        view: usize,
    }
    impl Mapping {
        fn open(write: bool) -> Option<Self> {
            unsafe {
                let size = std::mem::size_of::<Shared>() as u32;
                let handle = if write {
                    CreateFileMappingW(
                        INVALID_HANDLE_VALUE,
                        ptr::null(),
                        PAGE_READWRITE,
                        0,
                        size,
                        string(MAPPING).as_ptr(),
                    )
                } else {
                    OpenFileMappingW(FILE_MAP_READ, 0, string(MAPPING).as_ptr())
                };
                if handle.is_null() {
                    return None;
                }
                let view = MapViewOfFile(
                    handle,
                    if write { FILE_MAP_WRITE } else { FILE_MAP_READ },
                    0,
                    0,
                    size as usize,
                );
                if view.Value.is_null() {
                    CloseHandle(handle);
                    return None;
                }
                Some(Self {
                    handle: handle as usize,
                    view: view.Value as usize,
                })
            }
        }
        fn data(&self) -> &Shared {
            unsafe { &*(self.view as *const Shared) }
        }
        fn write(
            &self,
            temperature: Option<f32>,
            power: Option<f32>,
            fans: &[Option<u32>],
            fan_pct: Option<u32>,
            fan_chip: Option<&str>,
            status: u32,
        ) {
            let data = self.data();
            data.sequence.fetch_add(1, Ordering::SeqCst);
            data.magic.store(MAGIC, Ordering::Relaxed);
            data.temperature.store(
                temperature.map_or(f32::NAN, |v| v).to_bits(),
                Ordering::Relaxed,
            );
            data.power_watts
                .store(power.map_or(f32::NAN, |v| v).to_bits(), Ordering::Relaxed);
            for index in 0..FAN_SLOTS {
                let value = fans.get(index).and_then(|fan| *fan).unwrap_or(u32::MAX);
                data.fans[index].store(value, Ordering::Relaxed);
            }
            data.fan_pct
                .store(fan_pct.unwrap_or(u32::MAX), Ordering::Relaxed);
            // ASCII chip name, zero padded; ASCII keeps the reader simple.
            for (index, slot) in data.fan_chip.iter().enumerate() {
                let byte = fan_chip
                    .and_then(|name| name.as_bytes().get(index))
                    .copied()
                    .unwrap_or(0);
                slot.store(u32::from(byte), Ordering::Relaxed);
            }
            data.status.store(status, Ordering::Relaxed);
            data.timestamp.store(now_ms(), Ordering::Relaxed);
            data.sequence.fetch_add(1, Ordering::Release);
        }
    }
    impl Drop for Mapping {
        fn drop(&mut self) {
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.view as *mut c_void,
                });
                CloseHandle(self.handle as HANDLE);
            }
        }
    }

    /// CPU package power from the running energy accumulator. The unit comes
    /// from MSRC001_0299 once; watts are the counter delta over measured time.
    struct PackagePower {
        unit: Option<f64>,
        last_energy: Option<u32>,
        last_time: Option<std::time::Instant>,
        watts: Option<f32>,
    }
    impl PackagePower {
        fn new() -> Self {
            Self {
                unit: None,
                last_energy: None,
                last_time: None,
                watts: None,
            }
        }
        fn update(&mut self, cpu: &crate::pawn::PawnModule) -> Option<f32> {
            if self.unit.is_none() {
                let mut out = [0u64; 1];
                if cpu.execute("ioctl_read_msr", &[MSR_PWR_UNIT], &mut out) {
                    let esu = ((out[0] >> 8) & 0x1F) as i32;
                    // 2^-esu joules per increment (15.3 µJ at the default 16).
                    self.unit = Some(0.5f64.powi(esu));
                }
            }
            let unit = self.unit?;
            let mut out = [0u64; 1];
            if !cpu.execute("ioctl_read_msr", &[MSR_PKG_ENERGY_STAT], &mut out) {
                return self.watts;
            }
            let energy = out[0] as u32;
            if let (Some(last), Some(started)) = (self.last_energy, self.last_time) {
                let delta = if energy >= last {
                    energy - last
                } else {
                    (0xFFFF_FFFF - last) + energy
                };
                let seconds = started.elapsed().as_secs_f64();
                if delta > 0 && seconds > 0.5 {
                    self.watts = Some((unit * delta as f64 / seconds) as f32);
                }
            }
            self.last_energy = Some(energy);
            self.last_time = Some(std::time::Instant::now());
            self.watts
        }
    }

    pub struct Reader {
        mapping: Option<Mapping>,
    }
    impl Reader {
        pub fn new() -> Self {
            Self { mapping: None }
        }
        fn shared(&mut self) -> Option<&Shared> {
            if self.mapping.is_none() {
                self.mapping = Mapping::open(false);
            }
            self.mapping.as_ref().map(|m| m.data())
        }
        pub fn sample(&mut self) -> Option<f32> {
            let data = self.shared()?;
            let before = data.sequence.load(Ordering::Acquire);
            if before & 1 != 0 || data.magic.load(Ordering::Relaxed) != MAGIC {
                return None;
            }
            let timestamp = data.timestamp.load(Ordering::Relaxed);
            let value = f32::from_bits(data.temperature.load(Ordering::Relaxed));
            let status = data.status.load(Ordering::Relaxed);
            let after = data.sequence.load(Ordering::Acquire);
            validate_sample(before, after, status, timestamp, value, now_ms())
        }
        pub fn power_watts(&mut self) -> Option<f32> {
            let data = self.shared()?;
            let before = data.sequence.load(Ordering::Acquire);
            if before & 1 != 0 || data.magic.load(Ordering::Relaxed) != MAGIC {
                return None;
            }
            let value = f32::from_bits(data.power_watts.load(Ordering::Relaxed));
            let after = data.sequence.load(Ordering::Acquire);
            (before == after
                && data.status.load(Ordering::Relaxed) == 0
                && now_ms().abs_diff(data.timestamp.load(Ordering::Relaxed)) <= 8000
                && value.is_finite()
                && value >= 0.0
                && value < 1000.0)
                .then_some(value)
        }
        pub fn fans(&mut self) -> Vec<Option<u32>> {
            let Some(data) = self.shared() else {
                return Vec::new();
            };
            let before = data.sequence.load(Ordering::Acquire);
            if before & 1 != 0 || data.magic.load(Ordering::Relaxed) != MAGIC {
                return Vec::new();
            }
            let mut fans = Vec::with_capacity(FAN_SLOTS);
            for index in 0..FAN_SLOTS {
                let value = data.fans[index].load(Ordering::Relaxed);
                fans.push((value != u32::MAX).then_some(value));
            }
            while fans.last() == Some(&None) {
                fans.pop();
            }
            let after = data.sequence.load(Ordering::Acquire);
            if before != after
                || data.status.load(Ordering::Relaxed) != 0
                || now_ms().abs_diff(data.timestamp.load(Ordering::Relaxed)) > 8000
            {
                return Vec::new();
            }
            fans
        }
        pub fn fan_pct(&mut self) -> Option<u32> {
            let data = self.shared()?;
            let before = data.sequence.load(Ordering::Acquire);
            if before & 1 != 0 || data.magic.load(Ordering::Relaxed) != MAGIC {
                return None;
            }
            let value = data.fan_pct.load(Ordering::Relaxed);
            let timestamp = data.timestamp.load(Ordering::Relaxed);
            let status = data.status.load(Ordering::Relaxed);
            let after = data.sequence.load(Ordering::Acquire);
            (before == after && status == 0 && now_ms().abs_diff(timestamp) <= 8000 && value <= 100)
                .then_some(value)
        }
        pub fn chip_name(&mut self) -> Option<String> {
            let data = self.shared()?;
            let before = data.sequence.load(Ordering::Acquire);
            if before & 1 != 0 || data.magic.load(Ordering::Relaxed) != MAGIC {
                return None;
            }
            let bytes: Vec<u8> = data
                .fan_chip
                .iter()
                .map(|slot| slot.load(Ordering::Relaxed) as u8)
                .take_while(|byte| *byte != 0)
                .collect();
            let after = data.sequence.load(Ordering::Acquire);
            if before != after || bytes.is_empty() {
                return None;
            }
            String::from_utf8(bytes).ok().filter(|name| name.is_ascii())
        }
    }

    /// SMN register read behind the shared PCI mutex; other monitors take the
    /// same lock, so a busy lock simply skips this cycle's sample.
    unsafe fn read_smn_temperature(amd: &crate::pawn::PawnModule, pci: HANDLE) -> Option<f32> {
        if pci.is_null() {
            return None;
        }
        let wait = WaitForSingleObject(pci, 10);
        let taken = wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED;
        if !taken {
            return None;
        }
        let mut out = [0u64; 1];
        let value = if amd.execute("ioctl_read_smn", &[0x0005_9800], &mut out) {
            decode_zen4_temperature(out[0] as u32)
        } else {
            None
        };
        ReleaseMutex(pci);
        value
    }

    /// Loads the two signed modules and runs the sampling loop. The main tray
    /// owns the session mutex; when it exits, this helper exits with it.
    pub fn run_helper() -> Result<(), String> {
        unsafe {
            let lease = OpenMutexW(
                SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE,
                0,
                string(crate::startup::LEASE_NAME).as_ptr(),
            );
            if lease.is_null() {
                return Ok(());
            }
            let singleton = CreateMutexW(
                ptr::null(),
                1,
                string("Local\\LiquidDeck.CpuSensor.Writer.v1").as_ptr(),
            );
            if singleton.is_null() {
                CloseHandle(lease);
                return Err("Sensör kilidi oluşturulamadı".into());
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(singleton);
                CloseHandle(lease);
                return Ok(());
            }
            let mut pci: HANDLE = ptr::null_mut();
            let result = (|| {
                let mapping = Mapping::open(true).ok_or("Sensör paylaşımı açılamadı")?;
                mapping.data().sequence.store(0, Ordering::Release);
                let amd = crate::pawn::PawnModule::load(
                    include_bytes!("../assets/AMDFamily17.bin"),
                    "AMDFamily17",
                );
                let mut power = PackagePower::new();
                let mut superio = SuperIoProbe::new().ok();
                // SMN register reads share the PCI bus with FanControl and
                // LibreHardwareMonitor; LHM guards them with this mutex.
                pci = CreateMutexW(ptr::null(), 0, string(PCI_MUTEX).as_ptr());
                loop {
                    match WaitForSingleObject(lease, 0) {
                        WAIT_TIMEOUT => {}
                        WAIT_OBJECT_0 | WAIT_ABANDONED => {
                            ReleaseMutex(lease);
                            break;
                        }
                        _ => break,
                    }
                    let temperature = amd
                        .as_ref()
                        .ok()
                        .and_then(|amd| read_smn_temperature(amd, pci));
                    let power_watts = amd.as_ref().ok().and_then(|amd| power.update(amd));
                    let fan_sample = superio.as_mut().map(|probe| probe.sample());
                    let fans: Vec<Option<u32>> = fan_sample
                        .as_ref()
                        .map(|s| s.fans.clone())
                        .unwrap_or_default();
                    let fan_pct = fan_sample.as_ref().and_then(|sample| sample.fan_pct);
                    let fan_chip = fan_sample.as_ref().map(|s| s.name.clone());
                    mapping.write(
                        temperature,
                        power_watts,
                        &fans,
                        fan_pct,
                        fan_chip.as_deref(),
                        if temperature.is_some() { 0 } else { 1 },
                    );
                    match WaitForSingleObject(lease, 2000) {
                        WAIT_TIMEOUT => {}
                        WAIT_OBJECT_0 | WAIT_ABANDONED => {
                            ReleaseMutex(lease);
                            break;
                        }
                        _ => break,
                    }
                }
                if !pci.is_null() {
                    CloseHandle(pci);
                }
                mapping.write(None, None, &[], None, None, 2);
                Ok(())
            })();
            ReleaseMutex(singleton);
            CloseHandle(singleton);
            CloseHandle(lease);
            result
        }
    }
}

#[cfg(windows)]
pub use windows::{run_helper, Reader};
#[cfg(not(windows))]
pub struct Reader;
#[cfg(not(windows))]
impl Reader {
    pub fn new() -> Self {
        Self
    }
    pub fn sample(&mut self) -> Option<f32> {
        None
    }
    pub fn power_watts(&mut self) -> Option<f32> {
        None
    }
    pub fn fans(&mut self) -> Vec<Option<u32>> {
        Vec::new()
    }
    pub fn fan_pct(&mut self) -> Option<u32> {
        None
    }
    pub fn chip_name(&mut self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_zen4_register_without_substituting_zero() {
        assert_eq!(decode_zen4_temperature(400 << 21), Some(50.0));
        assert_eq!(decode_zen4_temperature((800 << 21) | 0x80000), Some(51.0));
        assert_eq!(decode_zen4_temperature(0), None);
        assert_eq!(decode_zen4_temperature(u32::MAX), None);
    }

    #[test]
    fn rejects_stale_torn_and_missing_sensor_samples() {
        assert_eq!(validate_sample(2, 2, 0, 10000, 51.5, 11000), Some(51.5));
        for (before, after, status, timestamp, value) in [
            (1, 1, 0, 10000, 50.0),
            (2, 4, 0, 10000, 50.0),
            (2, 2, 1, 10000, 50.0),
            (2, 2, 0, 1, 50.0),
            (2, 2, 0, 10000, f32::NAN),
            (2, 2, 0, 10000, 0.0),
            (2, 2, 0, 10000, 130.0),
        ] {
            assert_eq!(
                validate_sample(before, after, status, timestamp, value, 11000),
                None
            );
        }
    }
}
