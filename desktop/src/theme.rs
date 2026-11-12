pub const BG: u32 = 0x0E1512;
pub const SURFACE: u32 = 0x16211C;
pub const SURFACE_HI: u32 = 0x1F2E28;
pub const SURFACE_HOVER: u32 = 0x24352E;
pub const BORDER: u32 = 0x243A32;
pub const ACCENT: u32 = 0x8EE6B8;
pub const ACCENT_DIM: u32 = 0x33574A;
pub const TEXT: u32 = 0xE8F1EC;
pub const MUTED: u32 = 0x8FA39B;

pub fn fmt_time(ms: i64) -> String {
    let total = (ms.max(0) / 1000) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}
