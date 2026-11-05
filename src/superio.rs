//! Motherboard SuperIO fan-speed reader for the elevated sensor helper.
//! Ports LibreHardwareMonitor v0.9.6's register logic for the modern Nuvoton
//! (NCT67xx classic, NCT668x banked) and ITE IT87xx families onto the signed
//! PawnIO LpcIO module. Read-only: no fan control registers are ever written.
#![cfg(windows)]

use crate::pawn::PawnModule;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0},
    System::Threading::*,
};

const ISA_BUS_MUTEX: &str = "Global\\Access_ISABUS.HTP.Method";
const CHIP_ID_REGISTER: u8 = 0x20;
const CHIP_REVISION_REGISTER: u8 = 0x21;
const BASE_ADDRESS_REGISTER: u8 = 0x60;
const DEVICE_SELECT_REGISTER: u8 = 0x07;
const NUVOTON_HARDWARE_MONITOR_LDN: u8 = 0x0B;
const IT87_ENVIRONMENT_CONTROLLER_LDN: u8 = 0x04;
const NUVOTON_VENDOR_ID: u16 = 0x5CA3;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    NctClassic,
    Nct668x,
    ItE,
}

struct Chip {
    /// (chip id << 8) | revision, compared through `mask`.
    word: u16,
    mask: u16,
    name: &'static str,
    kind: Kind,
}

/// Modern desktop chips only; detection runs on the machine and reports the
/// exact model, so missing support degrades to "Veri yok", never fake data.
const CHIPS: &[Chip] = &[
    Chip {
        word: 0xD42B,
        mask: 0xFFFF,
        name: "Nuvoton NCT6798D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xD802,
        mask: 0xFFFF,
        name: "Nuvoton NCT6799D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xD423,
        mask: 0xFFFF,
        name: "Nuvoton NCT6796D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xD42A,
        mask: 0xFFFF,
        name: "Nuvoton NCT6796D-R",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xD451,
        mask: 0xFFFF,
        name: "Nuvoton NCT6797D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xD806,
        mask: 0xFFFF,
        name: "Nuvoton NCT6701D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xC560,
        mask: 0xF0FF,
        name: "Nuvoton NCT6779D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xC803,
        mask: 0xFFFF,
        name: "Nuvoton NCT6791D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xC911,
        mask: 0xFFFE,
        name: "Nuvoton NCT6792D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xD121,
        mask: 0xFFFF,
        name: "Nuvoton NCT6793D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xD352,
        mask: 0xFFFF,
        name: "Nuvoton NCT6795D",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xC330,
        mask: 0xF0FF,
        name: "Nuvoton NCT6776F",
        kind: Kind::NctClassic,
    },
    Chip {
        word: 0xD440,
        mask: 0xFFFE,
        name: "Nuvoton NCT6686D",
        kind: Kind::Nct668x,
    },
    Chip {
        word: 0xD592,
        mask: 0xFFFF,
        name: "Nuvoton NCT6687D",
        kind: Kind::Nct668x,
    },
    Chip {
        word: 0xC732,
        mask: 0xFFFF,
        name: "Nuvoton NCT6683D",
        kind: Kind::Nct668x,
    },
    Chip {
        word: 0x8620,
        mask: 0xFFFF,
        name: "ITE IT8620E",
        kind: Kind::ItE,
    },
    Chip {
        word: 0x8686,
        mask: 0xFFFF,
        name: "ITE IT8686E",
        kind: Kind::ItE,
    },
    Chip {
        word: 0x8689,
        mask: 0xFFFF,
        name: "ITE IT8689E",
        kind: Kind::ItE,
    },
    Chip {
        word: 0x8688,
        mask: 0xFFFF,
        name: "ITE IT8688E",
        kind: Kind::ItE,
    },
    Chip {
        word: 0x8665,
        mask: 0xFFFF,
        name: "ITE IT8665E",
        kind: Kind::ItE,
    },
    Chip {
        word: 0x8655,
        mask: 0xFFFF,
        name: "ITE IT8655E",
        kind: Kind::ItE,
    },
    Chip {
        word: 0x8628,
        mask: 0xFFFF,
        name: "ITE IT8628E",
        kind: Kind::ItE,
    },
    Chip {
        word: 0x8728,
        mask: 0xFFFF,
        name: "ITE IT8728F",
        kind: Kind::ItE,
    },
    Chip {
        word: 0x8792,
        mask: 0xFFFF,
        name: "ITE IT8792E",
        kind: Kind::ItE,
    },
];

struct SuperIo {
    name: &'static str,
    kind: Kind,
    base: u16,
}

pub struct SuperIoProbe {
    module: PawnModule,
    isa: usize,
    sio: Option<SuperIo>,
    done: bool,
    last_error: Option<String>,
}

pub struct FanSample {
    pub name: String,
    pub fans: Vec<Option<u32>>,
    /// Hardware-monitor PWM duty cycle for fan header 0, when supported.
    pub fan_pct: Option<u32>,
}

impl SuperIoProbe {
    pub fn new() -> Result<Self, String> {
        let module = PawnModule::load(include_bytes!("../assets/LpcIO.bin"), "LpcIO")?;
        let name = wide(ISA_BUS_MUTEX);
        unsafe {
            // FanControl/LibreHardwareMonitor coordinate SuperIO access with
            // this mutex; open theirs when present, otherwise create our own.
            let mut isa = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr());
            if isa.is_null() {
                isa = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            }
            if isa.is_null() {
                return Err("ISA veriyolu kilidi açılamadı".into());
            }
            Ok(Self {
                module,
                isa: isa as usize,
                sio: None,
                done: false,
                last_error: None,
            })
        }
    }

    /// Samples every connected fan header; `None` entries mean "no fan".
    /// The whole cycle shares the ISA bus mutex, like LibreHardwareMonitor.
    pub fn sample(&mut self) -> FanSample {
        if !self.done {
            self.detect();
            self.done = true;
        }
        let fallback = |error: Option<&String>| FanSample {
            name: error.map_or_else(|| "SuperIO bulunamadı".to_string(), |s| s.clone()),
            fans: Vec::new(),
            fan_pct: None,
        };
        let Some(sio) = self.sio.as_ref() else {
            return fallback(self.last_error.as_ref());
        };
        if !self.lock_bus() {
            return fallback(None);
        }
        let sample = match sio.kind {
            Kind::NctClassic => self.sample_nct_classic(sio),
            Kind::Nct668x => self.sample_nct668x(sio),
            Kind::ItE => self.sample_ite(sio),
        };
        self.unlock_bus();
        match sample {
            Ok((fans, fan_pct)) => FanSample {
                name: sio.name.to_string(),
                fans,
                fan_pct,
            },
            Err(error) => {
                self.last_error = Some(error);
                fallback(self.last_error.as_ref())
            }
        }
    }

    fn lock_bus(&self) -> bool {
        let wait = unsafe { WaitForSingleObject(self.isa as HANDLE, 100) };
        wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED
    }

    fn unlock_bus(&self) {
        unsafe {
            ReleaseMutex(self.isa as HANDLE);
        }
    }

    // ---- PawnIO LpcIO primitives (register/value port pairs) ----

    fn write_port(&self, port: u16, value: u8) {
        self.module.execute(
            "ioctl_pio_outb",
            &[u64::from(port), u64::from(value)],
            &mut [],
        );
    }

    fn read_port(&self, port: u16) -> u8 {
        let mut out = [0u64; 1];
        if self
            .module
            .execute("ioctl_pio_inb", &[u64::from(port)], &mut out)
        {
            out[0] as u8
        } else {
            0xFF
        }
    }

    fn sio_read_byte(&self, register: u8) -> u8 {
        let mut out = [0u64; 1];
        if self
            .module
            .execute("ioctl_superio_inb", &[u64::from(register)], &mut out)
        {
            out[0] as u8
        } else {
            0xFF
        }
    }

    fn sio_read_word(&self, register: u8) -> u16 {
        let mut out = [0u64; 1];
        if self
            .module
            .execute("ioctl_superio_inw", &[u64::from(register)], &mut out)
        {
            out[0] as u16
        } else {
            0xFFFF
        }
    }

    fn sio_write_byte(&self, register: u8, value: u8) {
        self.module.execute(
            "ioctl_superio_outb",
            &[u64::from(register), u64::from(value)],
            &mut [],
        );
    }

    fn find_bars(&self) {
        self.module.execute("ioctl_find_bars", &[], &mut []);
    }

    // ---- Detection (mirrors LibreHardwareMonitor LpcIO.Detect) ----

    fn detect(&mut self) {
        for (slot, reg_port, value_port) in [(0u64, 0x2Eu16, 0x2Fu16), (1, 0x4E, 0x4F)] {
            self.module.execute("ioctl_select_slot", &[slot], &mut []);
            if let Some(sio) = self
                .detect_nuvoton(reg_port, value_port)
                .or_else(|| self.detect_ite(reg_port, value_port))
            {
                self.sio = Some(sio);
                return;
            }
        }
        self.last_error = Some("Desteklenen fan çipi bulunamadı".into());
    }

    fn detect_nuvoton(&mut self, reg_port: u16, _value_port: u16) -> Option<SuperIo> {
        self.write_port(reg_port, 0x87);
        self.write_port(reg_port, 0x87);
        let id = self.sio_read_byte(CHIP_ID_REGISTER);
        let revision = self.sio_read_byte(CHIP_REVISION_REGISTER);
        let word = (u16::from(id) << 8) | u16::from(revision);
        let Some(chip) = CHIPS
            .iter()
            .find(|chip| chip.kind != Kind::ItE && word & chip.mask == chip.word)
        else {
            if id != 0 && id != 0xFF {
                self.write_port(reg_port, 0xAA);
            }
            return None;
        };

        self.find_bars();
        self.sio_write_byte(DEVICE_SELECT_REGISTER, NUVOTON_HARDWARE_MONITOR_LDN);
        let address = self.sio_read_word(BASE_ADDRESS_REGISTER);
        std::thread::sleep(std::time::Duration::from_millis(1));
        let verify = self.sio_read_word(BASE_ADDRESS_REGISTER);
        if address == verify && chip.kind == Kind::NctClassic {
            // Disable the hardware-monitor I/O space lock on 67x1/2/9x chips.
            let options = self.sio_read_byte(0x28);
            if options & 0x10 != 0 {
                self.sio_write_byte(0x28, options & !0x10);
            }
        }
        self.write_port(reg_port, 0xAA);

        if address != verify || address < 0x100 || address & 0xF007 != 0 {
            self.last_error = Some("Fan çipi adresi doğrulanamadı".into());
            return None;
        }
        let sio = SuperIo {
            name: chip.name,
            kind: chip.kind,
            base: address,
        };
        if chip.kind == Kind::NctClassic {
            // Vendor guard, read through the banked runtime interface.
            let high = u16::from(self.read_banked(sio.base, 0x804F));
            let low = u16::from(self.read_banked(sio.base, 0x004F));
            if high << 8 | low != NUVOTON_VENDOR_ID {
                self.last_error = Some("Nuvoton üretici kimliği doğrulanamadı".into());
                return None;
            }
        }
        Some(sio)
    }

    fn detect_ite(&mut self, reg_port: u16, value_port: u16) -> Option<SuperIo> {
        // IT87XX can enter only on 0x2E (0x4E hosts the secondary IT879x).
        self.write_port(reg_port, 0x87);
        self.write_port(reg_port, 0x01);
        self.write_port(reg_port, 0x55);
        self.write_port(reg_port, if reg_port == 0x4E { 0xAA } else { 0x55 });
        let id_word = self.sio_read_word(CHIP_ID_REGISTER);
        let Some(chip) = CHIPS
            .iter()
            .find(|chip| chip.kind == Kind::ItE && id_word & chip.mask == chip.word)
        else {
            return None;
        };

        self.find_bars();
        self.sio_write_byte(DEVICE_SELECT_REGISTER, IT87_ENVIRONMENT_CONTROLLER_LDN);
        let address = self.sio_read_word(BASE_ADDRESS_REGISTER);
        std::thread::sleep(std::time::Duration::from_millis(1));
        let verify = self.sio_read_word(BASE_ADDRESS_REGISTER);
        // Do not exit config mode on the secondary SuperIO (0x4E), like LHM.
        if reg_port != 0x4E {
            self.write_port(reg_port, 0x02);
            self.write_port(value_port, 0x02);
        }
        if address != verify || address < 0x100 || address & 0xF007 != 0 {
            self.last_error = Some("ITE adresi doğrulanamadı".into());
            return None;
        }
        Some(SuperIo {
            name: chip.name,
            kind: chip.kind,
            base: address,
        })
    }

    // ---- Runtime fan sampling ----

    fn read_banked(&self, base: u16, address: u16) -> u8 {
        self.write_port(base + 5, 0x4E);
        self.write_port(base + 6, (address >> 8) as u8);
        self.write_port(base + 5, (address & 0xFF) as u8);
        self.read_port(base + 6)
    }

    /// NCT668x uses a page/index/data handshake on its I/O window.
    fn read_paged(&self, base: u16, address: u16) -> u8 {
        const PAGE_SELECT: u8 = 0xFF;
        let page = base + 4;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while self.read_port(page) != PAGE_SELECT {
            if std::time::Instant::now() > deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        self.write_port(page, (address >> 8) as u8);
        self.write_port(base + 5, (address & 0xFF) as u8);
        let value = self.read_port(base + 6);
        self.write_port(page, PAGE_SELECT);
        value
    }

    fn sample_nct_classic(&self, sio: &SuperIo) -> Result<(Vec<Option<u32>>, Option<u32>), String> {
        // 13-bit fan count registers; each index is one motherboard header
        // (index 0 is the CPU fan on most boards).
        const FAN_COUNT_REGISTERS: [u16; 7] = [0x4B0, 0x4B2, 0x4B4, 0x4B6, 0x4B8, 0x4BA, 0x4CC];
        const MAX_COUNT: u32 = 0x1FFF;
        const MIN_COUNT: u32 = 0x15;
        let mut fans = Vec::with_capacity(FAN_COUNT_REGISTERS.len());
        for register in FAN_COUNT_REGISTERS {
            let high = self.read_banked(sio.base, register) as u32;
            let low = self.read_banked(sio.base, register + 1) as u32;
            let count = (high << 5) | (low & 0x1F);
            fans.push(if count >= MAX_COUNT {
                Some(0) // header present, fan stopped
            } else if count >= MIN_COUNT {
                Some((1_350_000.0 / count as f64).round() as u32)
            } else {
                None // no fan connected
            });
        }
        // NCT6797/6798/6799 duty-cycle sensors: 0x001 is fan header 0,
        // returned as 0..255 and scaled to percent by LibreHardwareMonitor.
        let fan_pct =
            Some(((self.read_banked(sio.base, 0x001) as f32 / 2.55).round() as u32).min(100));
        Ok((fans, fan_pct))
    }

    fn sample_nct668x(&self, sio: &SuperIo) -> Result<(Vec<Option<u32>>, Option<u32>), String> {
        let mut fans = Vec::with_capacity(8);
        for index in 0..8u16 {
            let register = 0x140 + 2 * index;
            let high = self.read_paged(sio.base, register) as u32;
            let low = self.read_paged(sio.base, register + 1) as u32;
            let rpm = (high << 8) | low;
            fans.push(Some(rpm));
        }
        Ok((fans, None))
    }

    fn sample_ite(&self, sio: &SuperIo) -> Result<(Vec<Option<u32>>, Option<u32>), String> {
        const TACH: [u16; 3] = [0x0d, 0x0e, 0x0f];
        const TACH_EXT: [u16; 3] = [0x18, 0x19, 0x1a];
        let sixteen_bit = self.read_banked(sio.base, 0x0C);
        let mut fans = Vec::with_capacity(3);
        for index in 0..3usize {
            let low = self.read_banked(sio.base, TACH[index]) as u32;
            if sixteen_bit & (1 << index) != 0 {
                let high = self.read_banked(sio.base, TACH_EXT[index]) as u32;
                let value = low | (high << 8);
                fans.push(if value < 0xFFFF {
                    Some((1_350_000.0 / (value * 2) as f64).round() as u32)
                } else {
                    Some(0)
                });
            } else if low < 0xFF {
                let divisors = self.read_banked(sio.base, 0x0B) as u32;
                let divisor = 1 << ((divisors >> (3 * index)) & 0x7);
                fans.push(Some((1_350_000.0 / (low * divisor) as f64).round() as u32));
            } else {
                fans.push(Some(0));
            }
        }
        Ok((fans, None))
    }
}

impl Drop for SuperIoProbe {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.isa as HANDLE);
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
